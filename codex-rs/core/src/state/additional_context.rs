use std::collections::BTreeMap;

use crate::context::AdditionalContextDeveloperFragment;
use crate::context::AdditionalContextUserFragment;
use crate::context::ContextualUserFragment;
use codex_protocol::models::ResponseInputItem;
use codex_protocol::protocol::AdditionalContextEntry;
use codex_protocol::protocol::AdditionalContextKind;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct AdditionalContextStore {
    values: BTreeMap<String, AdditionalContextEntry>,
}

impl AdditionalContextStore {
    pub(crate) fn render(&self) -> Vec<ResponseInputItem> {
        self.values.iter().map(Self::render_entry).collect()
    }

    pub(crate) fn merge(
        &mut self,
        values: BTreeMap<String, AdditionalContextEntry>,
    ) -> Vec<ResponseInputItem> {
        let fragments = values
            .iter()
            .filter(|(key, value)| self.values.get(*key) != Some(*value))
            .map(Self::render_entry)
            .collect();
        self.values = values;
        fragments
    }

    fn render_entry((key, entry): (&String, &AdditionalContextEntry)) -> ResponseInputItem {
        match entry.kind {
            AdditionalContextKind::Untrusted => {
                AdditionalContextUserFragment::new(key.clone(), entry.value.clone())
                    .into_response_input_item()
            }
            AdditionalContextKind::Application => {
                AdditionalContextDeveloperFragment::new(key.clone(), entry.value.clone())
                    .into_response_input_item()
            }
        }
    }
}
