extern crate proc_macro;
use proc_macro::{TokenStream, TokenTree, Delimiter, Group};

fn clean_stream(ts: TokenStream) -> TokenStream {
    let mut clean_tokens = Vec::new();
    let tokens: Vec<TokenTree> = ts.into_iter().collect();
    let mut i = 0;
    while i < tokens.len() {
        if let TokenTree::Punct(p) = &tokens[i] {
            if p.as_char() == '#' && i + 1 < tokens.len() {
                if let TokenTree::Group(g) = &tokens[i+1] {
                    let attr_tokens: Vec<TokenTree> = g.stream().into_iter().collect();
                    if attr_tokens.len() > 0 {
                        if let TokenTree::Ident(id) = &attr_tokens[0] {
                            if id.to_string() == "json" {
                                i += 2;
                                continue;
                            }
                        }
                    }
                }
            }
        }
        if let TokenTree::Group(g) = &tokens[i] {
            let inner_cleaned = clean_stream(g.stream());
            let mut new_g = Group::new(g.delimiter(), inner_cleaned);
            new_g.set_span(g.span());
            clean_tokens.push(TokenTree::Group(new_g));
        } else {
            clean_tokens.push(tokens[i].clone());
        }
        i += 1;
    }
    let mut out_ts = TokenStream::new();
    out_ts.extend(clean_tokens);
    out_ts
}

#[proc_macro_attribute]
pub fn axolote_json(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut struct_name = String::new();
    let mut fields_ts = None;

    let tokens: Vec<TokenTree> = item.clone().into_iter().collect();
    for (i, t) in tokens.iter().enumerate() {
        if let TokenTree::Ident(ident) = t {
            if ident.to_string() == "struct" {
                if let Some(TokenTree::Ident(name)) = tokens.get(i+1) {
                    struct_name = name.to_string();
                }
            }
        }
        if let TokenTree::Group(g) = t {
            if g.delimiter() == Delimiter::Brace {
                fields_ts = Some(g.stream());
            }
        }
    }

    if fields_ts.is_none() {
        return "compile_error!(\"Esperava os campos da struct dentro de {}\");".parse().unwrap();
    }

    let fields_tokens: Vec<TokenTree> = fields_ts.unwrap().into_iter().collect();
    
    struct Field {
        name: String,
        json_key: String, // Resolvido (name ou rename)
    }

    let mut parsed_fields = Vec::new();
    let mut current_ignore = false;
    let mut current_rename = None;

    let mut i = 0;
    while i < fields_tokens.len() {
        if let TokenTree::Punct(p) = &fields_tokens[i] {
            if p.as_char() == '#' {
                if i + 1 < fields_tokens.len() {
                    if let TokenTree::Group(g) = &fields_tokens[i+1] {
                        let attr_tokens: Vec<TokenTree> = g.stream().into_iter().collect();
                        if attr_tokens.len() > 0 {
                            if let TokenTree::Ident(id) = &attr_tokens[0] {
                                if id.to_string() == "json" && attr_tokens.len() > 1 {
                                    if let TokenTree::Group(inner_g) = &attr_tokens[1] {
                                        let ts_str = inner_g.stream().to_string();
                                        if ts_str.contains("ignore") {
                                            current_ignore = true;
                                        } else if ts_str.contains("rename") {
                                            let parts: Vec<&str> = ts_str.split('=').collect();
                                            if parts.len() == 2 {
                                                current_rename = Some(parts[1].trim().trim_matches('"').trim().to_string());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                i += 2;
                continue;
            }
        }

        if let TokenTree::Ident(id) = &fields_tokens[i] {
            if id.to_string() == "pub" {
                i += 1;
                continue;
            }
            if i + 1 < fields_tokens.len() {
                if let TokenTree::Punct(p) = &fields_tokens[i+1] {
                    if p.as_char() == ':' {
                        let field_name = id.to_string();
                        
                        if !current_ignore {
                            let json_key = match current_rename.take() {
                                Some(r) => format!("\"{}\"", r),
                                None => format!("\"{}\"", field_name),
                            };
                            parsed_fields.push(Field {
                                name: field_name,
                                json_key,
                            });
                        }
                        
                        current_ignore = false;
                        
                        while i < fields_tokens.len() {
                            if let TokenTree::Punct(cp) = &fields_tokens[i] {
                                if cp.as_char() == ',' {
                                    break;
                                }
                            }
                            i += 1;
                        }
                    }
                }
            }
        }
        i += 1;
    }

    let mut out = String::new();
    
    // Injeta os derives essenciais automaticamente no struct original
    out.push_str("#[derive(Default, Clone)]\n");
    let cleaned_item = clean_stream(item);
    out.push_str(&cleaned_item.to_string());
    
    // Implementa o trait ToJson
    out.push_str(&format!("\nimpl ::axolote::json::ToJson for {} {{\n", struct_name));
    out.push_str("    fn to_json_value(&self) -> ::axolote::json::JsonValue {\n");
    out.push_str("        let mut map = ::std::collections::HashMap::new();\n");
    for f in &parsed_fields {
        out.push_str(&format!("        map.insert({}.to_string(), ::axolote::json::ToJson::to_json_value(&self.{}));\n", f.json_key, f.name));
    }
    out.push_str("        ::axolote::json::JsonValue::Object(map)\n");
    out.push_str("    }\n}\n");

    // Implementa o trait FromJson
    out.push_str(&format!("impl ::axolote::json::FromJson for {} {{\n", struct_name));
    out.push_str("    fn from_json_value(val: &::axolote::json::JsonValue) -> ::std::result::Result<Self, String> {\n");
    out.push_str("        let mut obj = Self::default();\n");
    out.push_str("        ::axolote::json::FromJson::bind_json_value(&mut obj, val)?;\n");
    out.push_str("        Ok(obj)\n");
    out.push_str("    }\n");

    out.push_str("    fn bind_json_value(&mut self, val: &::axolote::json::JsonValue) -> ::std::result::Result<(), String> {\n");
    out.push_str("        if let ::axolote::json::JsonValue::Object(map) = val {\n");
    for f in &parsed_fields {
        out.push_str(&format!("            if let Some(json_field) = map.get({}) {{\n", f.json_key));
        out.push_str(&format!("                ::axolote::json::FromJson::bind_json_value(&mut self.{}, json_field)?;\n", f.name));
        out.push_str("            }\n");
    }
    out.push_str("            Ok(())\n");
    out.push_str("        } else {\n");
    out.push_str(&format!("            Err(format!(\"Esperava Object para {}\"))\n", struct_name));
    out.push_str("        }\n");
    out.push_str("    }\n}\n");

    // NOTA: A injeção automática de IntoBody foi removida daqui!
    // O usuário deve utilizar a struct Wrapper explícita: axolote::json::Json(struct)

    out.parse().unwrap()
}
