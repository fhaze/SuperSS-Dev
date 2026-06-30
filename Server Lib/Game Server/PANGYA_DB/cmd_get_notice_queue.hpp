// Arquivo cmd_get_notice_queue.hpp
// Definição da classe CmdGetNoticeQueue - polls notice_queue table

#pragma once
#ifndef _STDA_CMD_GET_NOTICE_QUEUE_HPP
#define _STDA_CMD_GET_NOTICE_QUEUE_HPP

#include "cmd_insert_ticker.hpp"
#include <vector>
#include <string>

namespace stdA {
	struct NoticeQueueEntry {
		uint32_t id;
		std::string message;
	};

	class CmdGetNoticeQueue : public pangya_db {
		public:
			explicit CmdGetNoticeQueue(bool _waiter = false);
			virtual ~CmdGetNoticeQueue();

			std::vector<NoticeQueueEntry>& getNotices() { return m_notices; };
			bool hasNotices() { return !m_notices.empty(); };

		protected:
			void lineResult(result_set::ctx_res* _result, uint32_t _index_result) override;
			response* prepareConsulta(database& _db) override;

			std::string _getName() override { return "CmdGetNoticeQueue"; };
			std::wstring _wgetName() override { return L"CmdGetNoticeQueue"; };

		private:
			std::vector<NoticeQueueEntry> m_notices;
			const char* m_szConsulta = "WITH updated AS (UPDATE pangya.notice_queue SET processed = 1 WHERE processed = 0 RETURNING id, message) SELECT id, message FROM updated LIMIT 5";
	};
}

#endif // !_STDA_CMD_GET_NOTICE_QUEUE_HPP
