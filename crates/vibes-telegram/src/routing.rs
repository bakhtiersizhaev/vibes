use vibes_core::{ChatScope, TelegramScopeHint};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TelegramChatKind {
    Direct,
    Group,
    Supergroup,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelegramEnvelope {
    pub chat_id: i64,
    pub chat_kind: TelegramChatKind,
    pub is_forum: bool,
    pub message_thread_id: Option<i64>,
}

pub fn resolve_scope(envelope: &TelegramEnvelope) -> ChatScope {
    ChatScope::from_telegram(TelegramScopeHint {
        chat_id: envelope.chat_id,
        is_direct: matches!(envelope.chat_kind, TelegramChatKind::Direct),
        is_forum: envelope.is_forum,
        message_thread_id: envelope.message_thread_id,
    })
}

#[cfg(test)]
mod tests {
    use vibes_core::ChatScope;

    use crate::routing::{TelegramChatKind, TelegramEnvelope, resolve_scope};

    #[test]
    fn direct_messages_map_to_direct_scope() {
        let scope = resolve_scope(&TelegramEnvelope {
            chat_id: 1668851955,
            chat_kind: TelegramChatKind::Direct,
            is_forum: false,
            message_thread_id: None,
        });

        assert_eq!(scope, ChatScope::Direct(1668851955));
    }

    #[test]
    fn forum_topics_map_to_topic_scope() {
        let scope = resolve_scope(&TelegramEnvelope {
            chat_id: -1003562096175,
            chat_kind: TelegramChatKind::Supergroup,
            is_forum: true,
            message_thread_id: Some(114),
        });

        assert_eq!(
            scope,
            ChatScope::Topic {
                chat_id: -1003562096175,
                topic_id: 114,
            }
        );
    }

    #[test]
    fn plain_groups_stay_group_scoped() {
        let scope = resolve_scope(&TelegramEnvelope {
            chat_id: -1003562096175,
            chat_kind: TelegramChatKind::Supergroup,
            is_forum: false,
            message_thread_id: Some(114),
        });

        assert_eq!(scope, ChatScope::Group(-1003562096175));
    }
}
