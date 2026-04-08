use serde::{Deserialize, Serialize};

pub type ChatId = i64;
pub type TopicId = i64;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChatScope {
    Direct(ChatId),
    Group(ChatId),
    Topic { chat_id: ChatId, topic_id: TopicId },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelegramScopeHint {
    pub chat_id: ChatId,
    pub is_direct: bool,
    pub is_forum: bool,
    pub message_thread_id: Option<TopicId>,
}

impl ChatScope {
    pub fn from_telegram(hint: TelegramScopeHint) -> Self {
        if hint.is_direct {
            return Self::Direct(hint.chat_id);
        }

        match (hint.is_forum, hint.message_thread_id) {
            (true, Some(topic_id)) => Self::Topic {
                chat_id: hint.chat_id,
                topic_id,
            },
            _ => Self::Group(hint.chat_id),
        }
    }

    pub fn scope_key(&self) -> String {
        match self {
            Self::Direct(chat_id) => format!("dm:{chat_id}"),
            Self::Group(chat_id) => format!("group:{chat_id}"),
            Self::Topic { chat_id, topic_id } => format!("topic:{chat_id}:{topic_id}"),
        }
    }

    pub fn topic_id(&self) -> Option<TopicId> {
        match self {
            Self::Topic { topic_id, .. } => Some(*topic_id),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ChatScope, TelegramScopeHint};

    #[test]
    fn resolves_direct_chat_scope() {
        let scope = ChatScope::from_telegram(TelegramScopeHint {
            chat_id: 42,
            is_direct: true,
            is_forum: false,
            message_thread_id: None,
        });

        assert_eq!(scope, ChatScope::Direct(42));
        assert_eq!(scope.scope_key(), "dm:42");
    }

    #[test]
    fn resolves_group_scope_without_topic() {
        let scope = ChatScope::from_telegram(TelegramScopeHint {
            chat_id: -100,
            is_direct: false,
            is_forum: true,
            message_thread_id: None,
        });

        assert_eq!(scope, ChatScope::Group(-100));
        assert_eq!(scope.scope_key(), "group:-100");
    }

    #[test]
    fn resolves_forum_topic_scope() {
        let scope = ChatScope::from_telegram(TelegramScopeHint {
            chat_id: -100,
            is_direct: false,
            is_forum: true,
            message_thread_id: Some(17),
        });

        assert_eq!(
            scope,
            ChatScope::Topic {
                chat_id: -100,
                topic_id: 17,
            }
        );
        assert_eq!(scope.scope_key(), "topic:-100:17");
    }
}
