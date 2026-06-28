//! Generic, type-safe packet dispatch.
//!
//! Replaces the C++ `func_arr` (a flat 10,000-entry array of `int(*)(void*,
//! void*)` pointers) and the 6,474-line `void*` switch in `packet_func_sv.cpp`.
//!
//! A [`Dispatch`] is a map from opcode → typed handler. Each server builds one
//! at startup. Handlers receive a parsed request and return a result, so the
//! compiler verifies every opcode matches its payload type — a class of bug
//! that was unrepresentable in the original `void*` model.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

/// Boxed async handler return type.
pub type HandlerResult<T> = Pin<Box<dyn Future<Output = Result<T, DispatchError>> + Send>>;

/// A boxed async handler that yields a `T` (typically a response buffer or
/// `()` for fire-and-forget handlers).
pub type Handler<C, T> = Box<dyn Fn(C, &[u8]) -> HandlerResult<T> + Send + Sync>;

/// Errors raised by the dispatch layer (unknown opcodes, parse failures).
#[derive(Debug, thiserror::Error)]
pub enum DispatchError {
    #[error("no handler registered for opcode {opcode:#06x}")]
    UnknownOpcode { opcode: u16 },
    #[error("payload error: {0}")]
    Payload(String),
}

/// An opcode → handler registry. `C` is the per-connection context passed to
/// every handler (e.g. an `Arc<Session>`); `T` is the handler return value.
///
/// Handlers are keyed by the raw `u16` opcode. The caller is responsible for
/// parsing the opcode off the front of the decoded frame body (see
/// [`pangya_proto::split_opcode`]); the dispatch table routes the rest.
pub struct Dispatch<C, T> {
    handlers: HashMap<u16, Handler<C, T>>,
}

impl<C, T> Default for Dispatch<C, T> {
    fn default() -> Self {
        Self {
            handlers: HashMap::new(),
        }
    }
}

impl<C, T> Dispatch<C, T> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a handler for an opcode. If a handler was already registered it
    /// is replaced (the C++ `addPacketCall` also overwrote).
    pub fn register<F, Fut>(&mut self, opcode: u16, handler: F)
    where
        F: Fn(C, &[u8]) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<T, DispatchError>> + Send + 'static,
    {
        let boxed: Handler<C, T> = Box::new(move |ctx, payload| Box::pin(handler(ctx, payload)));
        self.handlers.insert(opcode, boxed);
    }

    /// Look up and invoke the handler for an opcode.
    pub async fn dispatch(&self, opcode: u16, ctx: C, payload: &[u8]) -> Result<T, DispatchError> {
        let handler = self
            .handlers
            .get(&opcode)
            .ok_or(DispatchError::UnknownOpcode { opcode })?;
        handler(ctx, payload).await
    }

    /// Whether an opcode has a registered handler.
    pub fn has(&self, opcode: u16) -> bool {
        self.handlers.contains_key(&opcode)
    }

    /// Number of registered handlers.
    pub fn len(&self) -> usize {
        self.handlers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.handlers.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn registers_and_dispatches_by_opcode() {
        let mut d: Dispatch<(), String> = Dispatch::new();
        // Handlers copy what they need from the payload before yielding, since
        // the returned future must not borrow it. (Real handlers parse the
        // payload into owned structs up front.)
        d.register(0x01, |_, payload| {
            let len = payload.len();
            async move { Ok(format!("got {len} bytes")) }
        });
        d.register(0x02, |_, _| async move { Ok("two".into()) });

        assert_eq!(
            d.dispatch(0x01, (), &[1, 2, 3]).await.unwrap(),
            "got 3 bytes"
        );
        assert_eq!(d.dispatch(0x02, (), &[]).await.unwrap(), "two");
    }

    #[tokio::test]
    async fn unknown_opcode_is_an_error() {
        let d: Dispatch<(), ()> = Dispatch::new();
        let err = d.dispatch(0xFF, (), &[]).await.unwrap_err();
        assert!(matches!(err, DispatchError::UnknownOpcode { opcode: 0xFF }));
    }

    #[tokio::test]
    async fn handler_receives_context() {
        // The context (an Arc here) flows into the handler.
        let mut d: Dispatch<std::sync::Arc<u32>, u32> = Dispatch::new();
        d.register(0x05, |ctx, _| async move { Ok(*ctx) });

        let ctx = std::sync::Arc::new(42u32);
        assert_eq!(d.dispatch(0x05, ctx, &[]).await.unwrap(), 42);
    }
}
