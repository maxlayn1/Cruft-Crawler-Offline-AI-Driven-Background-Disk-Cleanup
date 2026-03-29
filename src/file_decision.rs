// Shared data type that flows from AI_MODEL → UI_ACTOR → FILE_HANDLER

use serde::{Serialize, Deserialize};
use crate::actor::crawler::FileMeta;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct FileDecision {
    pub meta:        FileMeta,
    pub ai_decision: String,
    pub ai_reason:   String,
}