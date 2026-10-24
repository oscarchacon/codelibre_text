//! Find and Replace panel - Modern editor-style find/replace

#[derive(Debug, Clone)]
pub struct FindReplacePanel {
    pub visible: bool,
    pub find_text: String,
    pub replace_text: String,
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub use_regex: bool,
    pub regex_pattern: Option<regex::Regex>,
    pub matches: Vec<MatchResult>,
    pub current_match_index: usize,
    pub search_direction: SearchDirection,
    pub show_replace: bool,
    pub match_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchDirection {
    Forward,
    Backward,
}

#[derive(Debug, Clone)]
pub struct MatchResult {
    pub start: usize,
    pub end: usize,
    pub matched_text: String,
}

impl FindReplacePanel {
    pub fn new() -> Self {
        Self {
            visible: false,
            find_text: String::new(),
            replace_text: String::new(),
            case_sensitive: false,
            whole_word: false,
            use_regex: false,
            regex_pattern: None,
            matches: Vec::new(),
            current_match_index: 0,
            search_direction: SearchDirection::Forward,
            show_replace: false,
            match_count: 0,
        }
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        if !self.visible {
            self.matches.clear();
        }
    }

    pub fn toggle_replace(&mut self) {
        self.show_replace = !self.show_replace;
    }

    pub fn set_case_sensitive(&mut self, sensitive: bool) {
        self.case_sensitive = sensitive;
        self.recompile_regex();
    }

    pub fn set_whole_word(&mut self, whole: bool) {
        self.whole_word = whole;
        self.recompile_regex();
    }

    pub fn set_use_regex(&mut self, use_regex: bool) {
        self.use_regex = use_regex;
        self.recompile_regex();
    }

    pub fn compile_regex(&self) -> Option<regex::Regex> {
        if self.find_text.is_empty() || !self.use_regex {
            return None;
        }

        let pattern = if self.whole_word {
            format!(r"\b{}\b", self.find_text)
        } else {
            self.find_text.clone()
        };

        let mut builder = regex::RegexBuilder::new(&pattern);
        builder.case_insensitive(!self.case_sensitive);
        builder.build().ok()
    }

    fn recompile_regex(&mut self) {
        self.regex_pattern = self.compile_regex();
    }

    /// Find all matches in the given text
    pub fn find_matches(&self, text: &str) -> Vec<MatchResult> {
        let mut matches = Vec::new();

        if self.find_text.is_empty() {
            return matches;
        }

        if self.use_regex {
            if let Some(regex) = &self.regex_pattern {
                for m in regex.find_iter(text) {
                    matches.push(MatchResult {
                        start: m.start(),
                        end: m.end(),
                        matched_text: m.as_str().to_string(),
                    });
                }
            }
        } else {
            let search_text = if self.case_sensitive {
                &self.find_text
            } else {
                &self.find_text.to_lowercase()
            };

            let text_to_search: String = if self.case_sensitive {
                text.to_string()
            } else {
                text.to_lowercase()
            };

            let mut search_start = 0;
            while let Some(pos) = text_to_search[search_start..].find(search_text) {
                let abs_pos = search_start + pos;
                let match_len = if self.whole_word {
                    search_text.len()
                } else {
                    search_text.len()
                };

                if abs_pos + match_len <= text.len() {
                    matches.push(MatchResult {
                        start: abs_pos,
                        end: abs_pos + match_len,
                        matched_text: text[abs_pos..abs_pos + match_len].to_string(),
                    });
                }

                search_start = abs_pos + 1;
                if search_start >= text.len() {
                    break;
                }
            }
        }

        matches
    }

    /// Replace first match
    pub fn replace_first(&self, text: &str) -> String {
        if self.matches.is_empty() {
            return text.to_string();
        }

        if let Some(m) = self.matches.get(self.current_match_index) {
            let mut result = String::new();
            result.push_str(&text[..m.start]);
            result.push_str(&self.replace_text);
            result.push_str(&text[m.end..]);
            result
        } else {
            text.to_string()
        }
    }

    /// Replace all matches
    pub fn replace_all(&self, text: &str) -> String {
        if self.matches.is_empty() {
            return text.to_string();
        }

        let mut result = text.to_string();
        let mut offset = 0;

        // Sort matches in reverse order to replace from end to start
        let mut sorted_matches: Vec<_> = self.matches.iter().collect();
        sorted_matches.sort_by(|a, b| b.start.cmp(&a.start));

        for m in sorted_matches {
            result.replace_range(m.start..m.end, &self.replace_text);
        }

        result
    }

    pub fn next_match(&mut self) {
        if self.matches.is_empty() {
            return;
        }
        self.current_match_index = (self.current_match_index + 1) % self.matches.len();
    }

    pub fn previous_match(&mut self) {
        if self.matches.is_empty() {
            return;
        }
        self.current_match_index = if self.current_match_index == 0 {
            self.matches.len() - 1
        } else {
            self.current_match_index - 1
        };
    }
}
