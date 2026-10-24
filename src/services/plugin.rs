//! Plugin system - Extension API for CodeLibre-style editors

use std::collections::HashMap;
use uuid::Uuid;

/// Plugin event types (30+ event types)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PluginEvent {
    OnInit,
    OnNew,
    OnClone,
    OnLoad,
    OnRevert,
    OnReload,
    OnPreClose,
    OnClose,
    OnPreSave,
    OnPostSave,
    OnPreMove,
    OnPostMove,
    OnModified,
    OnSelectionModified,
    OnActivated,
    OnDeactivated,
    OnQueryContext,
    OnQueryCompletions,
    OnHover,
    OnTextCommand,
    OnWindowCommand,
    OnPostTextCommand,
    OnPostWindowCommand,
    OnModifiedAsync,
    OnSelectionModifiedAsync,
    OnPreSaveAsync,
    OnPostSaveAsync,
    OnActivatedAsync,
    OnDeactivatedAsync,
    OnNewBuffer,
    OnNewBufferAsync,
    OnAssociateBuffer,
    OnAssociateBufferAsync,
    OnCloseBuffer,
    OnCloseBufferAsync,
    OnNewProject,
    OnNewProjectAsync,
    OnLoadProject,
    OnLoadProjectAsync,
    OnPreCloseProject,
    OnPreCloseWindow,
    OnExit,
}

/// Completion item for auto-complete
#[derive(Debug, Clone)]
pub struct CompletionItem {
    pub trigger: String,
    pub contents: String,
    pub annotation: String,
    pub kind: CompletionKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionKind {
    Snippet,
    Keyword,
    Type,
    Function,
    Namespace,
    Navigation,
    Markup,
    Variable,
    Color,
}

impl CompletionKind {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Snippet => "snippet",
            Self::Keyword => "keyword",
            Self::Type => "type",
            Self::Function => "function",
            Self::Namespace => "namespace",
            Self::Navigation => "navigation",
            Self::Markup => "markup",
            Self::Variable => "variable",
            Self::Color => "color",
        }
    }
}

/// Plugin info
#[derive(Debug, Clone)]
pub struct PluginInfo {
    pub id: Uuid,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub enabled: bool,
}

/// Plugin system
pub struct PluginManager {
    pub plugins: HashMap<Uuid, PluginInfo>,
    pub event_handlers: HashMap<PluginEvent, Vec<Box<dyn Fn()>>>,
    pub completion_providers: HashMap<Uuid, Vec<Box<dyn Fn(&str) -> Vec<CompletionItem>>>>,
    pub context_query_handlers: HashMap<Uuid, Vec<Box<dyn Fn(&str) -> bool>>>,
}

impl std::fmt::Debug for PluginManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginManager")
            .field("plugins", &self.plugins)
            .finish_non_exhaustive()
    }
}

impl PluginManager {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
            event_handlers: HashMap::new(),
            completion_providers: HashMap::new(),
            context_query_handlers: HashMap::new(),
        }
    }

    pub fn register_plugin(&mut self, plugin: PluginInfo) -> Uuid {
        let id = plugin.id;
        self.plugins.insert(id, plugin);
        id
    }

    pub fn unregister_plugin(&mut self, id: Uuid) {
        self.plugins.remove(&id);
    }

    pub fn enable_plugin(&mut self, id: Uuid) {
        if let Some(plugin) = self.plugins.get_mut(&id) {
            plugin.enabled = true;
        }
    }

    pub fn disable_plugin(&mut self, id: Uuid) {
        if let Some(plugin) = self.plugins.get_mut(&id) {
            plugin.enabled = false;
        }
    }

    pub fn is_enabled(&self, id: Uuid) -> bool {
        self.plugins.get(&id).map(|p| p.enabled).unwrap_or(false)
    }

    /// Register an event handler
    pub fn on(&mut self, event: PluginEvent, handler: Box<dyn Fn()>) {
        self.event_handlers
            .entry(event)
            .or_insert_with(Vec::new)
            .push(handler);
    }

    /// Trigger an event
    pub fn trigger_event(&self, event: &PluginEvent) {
        if let Some(handlers) = self.event_handlers.get(event) {
            for handler in handlers {
                handler();
            }
        }
    }

    /// Register a completion provider
    pub fn register_completion_provider(&mut self, plugin_id: Uuid, provider: Box<dyn Fn(&str) -> Vec<CompletionItem>>) {
        self.completion_providers
            .entry(plugin_id)
            .or_insert_with(Vec::new)
            .push(provider);
    }

    /// Get completions for a trigger
    pub fn get_completions(&self, trigger: &str) -> Vec<CompletionItem> {
        let mut completions = Vec::new();
        for providers in self.completion_providers.values() {
            for provider in providers {
                completions.extend(provider(trigger));
            }
        }
        completions
    }

    /// Query context
    pub fn query_context(&self, context: &str, key: &str, op: &str, value: &str) -> bool {
        for handlers in self.context_query_handlers.values() {
            for handler in handlers {
                if handler(context) {
                    return true;
                }
            }
        }
        false
    }

    /// List all plugins
    pub fn list_plugins(&self) -> Vec<&PluginInfo> {
        self.plugins.values().collect()
    }
}
