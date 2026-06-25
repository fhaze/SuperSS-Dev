// Arquivo cmd_get_notice_queue.cpp
// Implementação da classe CmdGetNoticeQueue

#include "cmd_get_notice_queue.hpp"

using namespace stdA;

CmdGetNoticeQueue::CmdGetNoticeQueue(bool _waiter) : pangya_db(_waiter) {
}

CmdGetNoticeQueue::~CmdGetNoticeQueue() {
}

void CmdGetNoticeQueue::lineResult(result_set::ctx_res* _result, uint32_t /*_index_result*/) {
	NoticeQueueEntry entry;
	entry.id = IFNULL(atoi, _result->at(0));
	if (_result->at(1) != nullptr)
		entry.message = _result->at(1);
	m_notices.push_back(entry);
}

response* CmdGetNoticeQueue::prepareConsulta(database& _db) {
	auto r = consulta(_db, m_szConsulta);
	checkResponse(r, "nao conseguiu consultar notice_queue");
	return r;
}
