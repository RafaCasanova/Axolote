use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum JsonValue {
    Null,
    Boolean(bool),
    Number(f64),
    String(String),
    Array(Vec<JsonValue>),
    Object(HashMap<String, JsonValue>),
}

impl fmt::Display for JsonValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JsonValue::Null => write!(f, "null"),
            JsonValue::Boolean(b) => write!(f, "{}", b),
            JsonValue::Number(n) => write!(f, "{}", n),
            JsonValue::String(s) => {
                // Escape simple characters
                let escaped = s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n").replace('\r', "\\r").replace('\t', "\\t");
                write!(f, "\"{}\"", escaped)
            }
            JsonValue::Array(arr) => {
                write!(f, "[")?;
                for (i, val) in arr.iter().enumerate() {
                    if i > 0 { write!(f, ",")?; }
                    write!(f, "{}", val)?;
                }
                write!(f, "]")
            }
            JsonValue::Object(obj) => {
                write!(f, "{{")?;
                let mut first = true;
                for (k, v) in obj {
                    if !first { write!(f, ",")?; }
                    let escaped_key = k.replace('\\', "\\\\").replace('"', "\\\"");
                    write!(f, "\"{}\":{}", escaped_key, v)?;
                    first = false;
                }
                write!(f, "}}")
            }
        }
    }
}

// Trait para converter Structs em JSON
pub trait ToJson {
    /// Converte a struct para uma string JSON formatada.
    fn to_json(&self) -> String {
        self.to_json_value().to_string()
    }

    /// Método interno gerado pela macro
    fn to_json_value(&self) -> JsonValue;
}

// Trait para popular Structs a partir de JSON
pub trait FromJson: Default {
    /// Faz o parse de uma string JSON para a struct.
    /// Em caso de falha (JSON inválido), retorna um erro indicando o problema.
    fn from_json(json_str: &str) -> Result<Self, String> {
        let tree = parse_json(json_str)?;
        Self::from_json_value(&tree)
    }

    /// Atualiza a struct atual com os valores da string JSON.
    /// Em caso de falha, retorna erro e mantém a struct inalterada.
    fn bind_json(&mut self, json_str: &str) -> Result<(), String> {
        let tree = parse_json(json_str)?;
        self.bind_json_value(&tree)
    }

    /// Método interno gerado pela macro
    fn from_json_value(val: &JsonValue) -> Result<Self, String>;
    
    /// Método interno gerado pela macro. Tem implementação padrão para tipos simples.
    fn bind_json_value(&mut self, val: &JsonValue) -> Result<(), String> {
        *self = Self::from_json_value(val)?;
        Ok(())
    }
}

// Implementações para tipos básicos

impl ToJson for String {
    fn to_json_value(&self) -> JsonValue {
        JsonValue::String(self.clone())
    }
}
impl FromJson for String {
    fn from_json_value(val: &JsonValue) -> Result<Self, String> {
        match val {
            JsonValue::String(s) => Ok(s.clone()),
            _ => Err("Esperava String".to_string()),
        }
    }
}

macro_rules! impl_json_for_num {
    ($($t:ty)*) => {
        $(
            impl ToJson for $t {
                fn to_json_value(&self) -> JsonValue {
                    JsonValue::Number(*self as f64)
                }
            }
            impl FromJson for $t {
                fn from_json_value(val: &JsonValue) -> Result<Self, String> {
                    match val {
                        JsonValue::Number(n) => Ok(*n as $t),
                        _ => Err(format!("Esperava {}", stringify!($t))),
                    }
                }
            }
        )*
    }
}
impl_json_for_num!(i8 i16 i32 i64 isize u8 u16 u32 u64 usize f32 f64);

impl ToJson for bool {
    fn to_json_value(&self) -> JsonValue {
        JsonValue::Boolean(*self)
    }
}
impl FromJson for bool {
    fn from_json_value(val: &JsonValue) -> Result<Self, String> {
        match val {
            JsonValue::Boolean(b) => Ok(*b),
            _ => Err("Esperava Boolean".to_string()),
        }
    }
}

impl<T: ToJson> ToJson for Vec<T> {
    fn to_json_value(&self) -> JsonValue {
        JsonValue::Array(self.iter().map(|item| item.to_json_value()).collect())
    }
}
impl<T: FromJson> FromJson for Vec<T> {
    fn from_json_value(val: &JsonValue) -> Result<Self, String> {
        match val {
            JsonValue::Array(arr) => {
                let mut vec = Vec::new();
                for item in arr {
                    vec.push(T::from_json_value(item)?);
                }
                Ok(vec)
            },
            _ => Err("Esperava Array".to_string()),
        }
    }
}

impl<T: ToJson> ToJson for Option<T> {
    fn to_json_value(&self) -> JsonValue {
        match self {
            Some(v) => v.to_json_value(),
            None => JsonValue::Null,
        }
    }
}
impl<T: FromJson> FromJson for Option<T> {
    fn from_json_value(val: &JsonValue) -> Result<Self, String> {
        match val {
            JsonValue::Null => Ok(None),
            _ => Ok(Some(T::from_json_value(val)?)),
        }
    }
}

// ==========================================
// PARSER JSON NATIVO (Zero Dependências)
// ==========================================

pub fn parse_json(input: &str) -> Result<JsonValue, String> {
    let mut chars: Vec<char> = input.chars().collect();
    let mut pos = 0;
    let val = parse_value(&mut chars, &mut pos)?;
    skip_whitespace(&mut chars, &mut pos);
    if pos < chars.len() {
        return Err(format!("Caracteres extras após o fim do JSON na posição {}", pos));
    }
    Ok(val)
}

fn skip_whitespace(chars: &[char], pos: &mut usize) {
    while *pos < chars.len() && chars[*pos].is_whitespace() {
        *pos += 1;
    }
}

fn parse_value(chars: &[char], pos: &mut usize) -> Result<JsonValue, String> {
    skip_whitespace(chars, pos);
    if *pos >= chars.len() {
        return Err("Fim de entrada inesperado".to_string());
    }

    match chars[*pos] {
        '{' => parse_object(chars, pos),
        '[' => parse_array(chars, pos),
        '"' => parse_string(chars, pos).map(JsonValue::String),
        't' => parse_true(chars, pos),
        'f' => parse_false(chars, pos),
        'n' => parse_null(chars, pos),
        c if c == '-' || c.is_digit(10) => parse_number(chars, pos),
        c => Err(format!("Caractere inesperado '{}' na posição {}", c, pos)),
    }
}

fn parse_object(chars: &[char], pos: &mut usize) -> Result<JsonValue, String> {
    *pos += 1; // skip '{'
    skip_whitespace(chars, pos);
    let mut map = HashMap::new();

    if *pos < chars.len() && chars[*pos] == '}' {
        *pos += 1;
        return Ok(JsonValue::Object(map));
    }

    loop {
        skip_whitespace(chars, pos);
        if *pos >= chars.len() || chars[*pos] != '"' {
            return Err("Esperava string de chave no objeto".to_string());
        }
        let key = parse_string(chars, pos)?;
        
        skip_whitespace(chars, pos);
        if *pos >= chars.len() || chars[*pos] != ':' {
            return Err("Esperava ':' após a chave no objeto".to_string());
        }
        *pos += 1; // skip ':'
        
        let val = parse_value(chars, pos)?;
        map.insert(key, val);
        
        skip_whitespace(chars, pos);
        if *pos >= chars.len() {
            return Err("Fim inesperado no objeto".to_string());
        }
        match chars[*pos] {
            ',' => {
                *pos += 1;
            }
            '}' => {
                *pos += 1;
                return Ok(JsonValue::Object(map));
            }
            c => return Err(format!("Esperava ',' ou '}}', mas encontrou '{}'", c)),
        }
    }
}

fn parse_array(chars: &[char], pos: &mut usize) -> Result<JsonValue, String> {
    *pos += 1; // skip '['
    skip_whitespace(chars, pos);
    let mut arr = Vec::new();

    if *pos < chars.len() && chars[*pos] == ']' {
        *pos += 1;
        return Ok(JsonValue::Array(arr));
    }

    loop {
        let val = parse_value(chars, pos)?;
        arr.push(val);
        
        skip_whitespace(chars, pos);
        if *pos >= chars.len() {
            return Err("Fim inesperado no array".to_string());
        }
        match chars[*pos] {
            ',' => {
                *pos += 1;
            }
            ']' => {
                *pos += 1;
                return Ok(JsonValue::Array(arr));
            }
            c => return Err(format!("Esperava ',' ou ']', mas encontrou '{}'", c)),
        }
    }
}

fn parse_string(chars: &[char], pos: &mut usize) -> Result<String, String> {
    *pos += 1; // skip initial '"'
    let mut s = String::new();
    while *pos < chars.len() {
        let c = chars[*pos];
        if c == '"' {
            *pos += 1;
            return Ok(s);
        } else if c == '\\' {
            *pos += 1;
            if *pos >= chars.len() {
                return Err("Fim inesperado após escape".to_string());
            }
            match chars[*pos] {
                '"' => s.push('"'),
                '\\' => s.push('\\'),
                '/' => s.push('/'),
                'b' => s.push('\x08'),
                'f' => s.push('\x0C'),
                'n' => s.push('\n'),
                'r' => s.push('\r'),
                't' => s.push('\t'),
                'u' => {
                    // Ignorando escape unicode por simplicidade para não usar dependências extras
                    *pos += 4; // avança 4 posições
                    s.push('?');
                }
                c => return Err(format!("Escape inválido: \\{}", c)),
            }
        } else {
            s.push(c);
        }
        *pos += 1;
    }
    Err("Fim inesperado dentro de string".to_string())
}

fn parse_true(chars: &[char], pos: &mut usize) -> Result<JsonValue, String> {
    if *pos + 4 <= chars.len() && &chars[*pos..*pos+4] == ['t', 'r', 'u', 'e'] {
        *pos += 4;
        Ok(JsonValue::Boolean(true))
    } else {
        Err("Esperava 'true'".to_string())
    }
}

fn parse_false(chars: &[char], pos: &mut usize) -> Result<JsonValue, String> {
    if *pos + 5 <= chars.len() && &chars[*pos..*pos+5] == ['f', 'a', 'l', 's', 'e'] {
        *pos += 5;
        Ok(JsonValue::Boolean(false))
    } else {
        Err("Esperava 'false'".to_string())
    }
}

fn parse_null(chars: &[char], pos: &mut usize) -> Result<JsonValue, String> {
    if *pos + 4 <= chars.len() && &chars[*pos..*pos+4] == ['n', 'u', 'l', 'l'] {
        *pos += 4;
        Ok(JsonValue::Null)
    } else {
        Err("Esperava 'null'".to_string())
    }
}

fn parse_number(chars: &[char], pos: &mut usize) -> Result<JsonValue, String> {
    let start = *pos;
    if chars[*pos] == '-' {
        *pos += 1;
    }
    while *pos < chars.len() && chars[*pos].is_digit(10) {
        *pos += 1;
    }
    if *pos < chars.len() && chars[*pos] == '.' {
        *pos += 1;
        while *pos < chars.len() && chars[*pos].is_digit(10) {
            *pos += 1;
        }
    }
    if *pos < chars.len() && (chars[*pos] == 'e' || chars[*pos] == 'E') {
        *pos += 1;
        if *pos < chars.len() && (chars[*pos] == '+' || chars[*pos] == '-') {
            *pos += 1;
        }
        while *pos < chars.len() && chars[*pos].is_digit(10) {
            *pos += 1;
        }
    }
    let num_str: String = chars[start..*pos].iter().collect();
    match num_str.parse::<f64>() {
        Ok(n) => Ok(JsonValue::Number(n)),
        Err(_) => Err(format!("Número inválido: {}", num_str)),
    }
}
