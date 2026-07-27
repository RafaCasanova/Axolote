use crate::http::HttpMethod;
use crate::route::HandlerFn;
use std::collections::HashMap;

pub struct RadixNode {
    pub handlers: HashMap<HttpMethod, HandlerFn>,
    pub static_children: HashMap<String, RadixNode>,
    pub param_children: Vec<RadixNode>,
    pub param_name: Option<String>,
    pub param_type: Option<String>,
}

impl RadixNode {
    pub fn new() -> Self {
        RadixNode {
            handlers: HashMap::new(),
            static_children: HashMap::new(),
            param_children: Vec::new(),
            param_name: None,
            param_type: None,
        }
    }
}

pub struct RadixTree {
    root: RadixNode,
}

impl RadixTree {
    pub fn new() -> Self {
        RadixTree {
            root: RadixNode::new(),
        }
    }

    pub fn insert(&mut self, method: HttpMethod, path: &str, handler: HandlerFn) {
        let mut current_node = &mut self.root;
        let parts = path.split('/').filter(|s| !s.is_empty());

        for part in parts {
            if part.starts_with('{') && part.ends_with('}') {
                let raw_key = &part[1..part.len() - 1];
                let (key, p_type) = match raw_key.find(':') {
                    Some(idx) => (&raw_key[..idx], Some(raw_key[idx + 1..].to_string())),
                    None => (raw_key, None),
                };

                let mut found_idx = None;
                for (i, child) in current_node.param_children.iter().enumerate() {
                    if child.param_name.as_deref() == Some(key) && child.param_type == p_type {
                        found_idx = Some(i);
                        break;
                    }
                }

                if let Some(idx) = found_idx {
                    current_node = &mut current_node.param_children[idx];
                } else {
                    let mut node = RadixNode::new();
                    node.param_name = Some(key.to_string());
                    node.param_type = p_type;
                    current_node.param_children.push(node);
                    let last_idx = current_node.param_children.len() - 1;
                    current_node = &mut current_node.param_children[last_idx];
                }
            } else {
                current_node = current_node
                    .static_children
                    .entry(part.to_string())
                    .or_insert_with(RadixNode::new);
            }
        }

        current_node.handlers.insert(method, handler);
    }

    pub fn find<'a>(
        &'a self,
        method: &HttpMethod,
        path: &str,
    ) -> Option<(&'a HandlerFn, HashMap<String, String>)> {
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        let mut params = HashMap::new();

        if let Some(node) = self.find_node(&self.root, &parts, 0, &mut params, method) {
            if let Some(handler) = node.handlers.get(method) {
                return Some((handler, params));
            }
        }
        None
    }

    fn find_node<'a>(
        &'a self,
        node: &'a RadixNode,
        parts: &[&str],
        index: usize,
        params: &mut HashMap<String, String>,
        method: &HttpMethod,
    ) -> Option<&'a RadixNode> {
        if index == parts.len() {
            if node.handlers.contains_key(method) {
                return Some(node);
            }
            return None;
        }

        let part = parts[index];

        // 1. Prioridade: Tenta rota estática exata
        if let Some(child) = node.static_children.get(part) {
            if let Some(found) = self.find_node(child, parts, index + 1, params, method) {
                return Some(found);
            }
        }

        // 2. Fallback: Tenta rotas parametrizadas
        for param_child in &node.param_children {
            let is_valid = match param_child.param_type.as_deref() {
                Some("num") => !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()),
                Some("alpha") => !part.is_empty() && part.chars().all(|c| c.is_ascii_alphabetic()),
                Some("alnum") => {
                    !part.is_empty() && part.chars().all(|c| c.is_ascii_alphanumeric())
                }
                _ => true, // default accepts anything
            };

            if is_valid {
                if let Some(name) = &param_child.param_name {
                    params.insert(name.clone(), part.to_string());
                }

                if let Some(found) = self.find_node(param_child, parts, index + 1, params, method) {
                    return Some(found);
                }

                // Backtracking se falhou
                if let Some(name) = &param_child.param_name {
                    params.remove(name);
                }
            }
        }

        None
    }
}
