use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::Path;

// Représentation de toutes les valeurs Nix possibles
#[derive(Debug, Clone, PartialEq)]
pub enum NixValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Path(String),
    List(Vec<NixValue>),
    AttrSet(HashMap<String, NixValue>),
    Function(Box<NixFunction>),
    Let(Box<NixLet>),
    With(Box<NixWith>),
    Inherit(Vec<String>),
    Import(String),
    Variable(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct NixFunction {
    pub params: Vec<String>,
    pub body: NixValue,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NixLet {
    pub bindings: HashMap<String, NixValue>,
    pub body: NixValue,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NixWith {
    pub expr: NixValue,
    pub body: NixValue,
}

// Erreur de parsing avec contexte
#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    pub line: usize,
    pub col: usize,
    pub context: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Erreur de parsing à la ligne {}, colonne {}:\n{}\nContexte: {}",
               self.line, self.col, self.message, self.context)
    }
}

impl std::error::Error for ParseError {}

// Parser de fichiers Nix
pub struct NixParser {
    input: Vec<char>,
    pos: usize,
    line: usize,
    col: usize,
}

impl NixParser {
    pub fn new(input: &str) -> Self {
        Self {
            input: input.chars().collect(),
            pos: 0,
            line: 1,
            col: 1,
        }
    }

    fn current(&self) -> Option<char> {
        self.input.get(self.pos).copied()
    }

    fn advance(&mut self) {
        if let Some(c) = self.current() {
            if c == '\n' {
                self.line += 1;
                self.col = 1;
            } else {
                self.col += 1;
            }
        }
        self.pos += 1;
    }

    fn get_context(&self, range: usize) -> String {
        let start = self.pos.saturating_sub(range);
        let end = (self.pos + range).min(self.input.len());
        let context: String = self.input[start..end].iter().collect();
        let pointer_pos = (self.pos - start).min(context.len());
        format!("{}\n{}^", context.replace('\n', "\\n"), " ".repeat(pointer_pos))
    }

    fn error(&self, msg: &str) -> ParseError {
        ParseError {
            message: msg.to_string(),
            line: self.line,
            col: self.col,
            context: self.get_context(30),
        }
    }

    fn skip_whitespace(&mut self) {
        while let Some(c) = self.current() {
            if c.is_whitespace() {
                self.advance();
            } else if c == '#' {
                // Commentaire ligne
                while let Some(ch) = self.current() {
                    self.advance();
                    if ch == '\n' {
                        break;
                    }
                }
            } else if self.peek_string("/*") {
                // Commentaire bloc
                self.advance();
                self.advance();
                while !self.peek_string("*/") && self.current().is_some() {
                    self.advance();
                }
                if self.peek_string("*/") {
                    self.advance();
                    self.advance();
                }
            } else {
                break;
            }
        }
    }

    fn peek_string(&self, s: &str) -> bool {
        let chars: Vec<char> = s.chars().collect();
        for (i, &ch) in chars.iter().enumerate() {
            if self.input.get(self.pos + i) != Some(&ch) {
                return false;
            }
        }
        true
    }

    fn parse_identifier(&mut self) -> Result<String, ParseError> {
        let mut id = String::new();
        while let Some(c) = self.current() {
            if c.is_alphanumeric() || c == '_' || c == '-' || c == '\'' {
                id.push(c);
                self.advance();
            } else {
                break;
            }
        }
        if id.is_empty() {
            Err(self.error("Expected identifier"))
        } else {
            Ok(id)
        }
    }

    fn parse_path(&mut self) -> Result<String, ParseError> {
        let mut path = String::new();

        // Gérer les chemins relatifs et absolus
        // Accepter: lettres, chiffres, _, -, /, .
        while let Some(c) = self.current() {
            match c {
                'a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-' | '/' | '.' => {
                    path.push(c);
                    self.advance();
                }
                _ => break,
            }
        }

        if path.is_empty() {
            Err(self.error("Expected path"))
        } else {
            Ok(path)
        }
    }

    fn parse_attribute_path(&mut self) -> Result<String, ParseError> {
        let mut path = String::new();

        loop {
            // Vérifier qu'on n'est pas sur un chemin de fichier
            if self.current() == Some('.') && self.input.get(self.pos + 1) == Some(&'/') {
                // C'est un chemin relatif, pas un accès d'attribut
                return Err(self.error("Path found where identifier expected"));
            }

            // Gérer les clés entre guillemets comme fileSystems."/".options
            let part = if self.current() == Some('"') {
                let s = self.parse_string()?;
                // Préserver les guillemets dans le chemin pour le reformatage
                format!("\"{}\"", s)
            } else {
                self.parse_identifier()?
            };

            path.push_str(&part);

            self.skip_whitespace();
            if self.current() == Some('.') {
                // Vérifier que le prochain caractère n'est pas '/' (ce serait un chemin)
                if self.input.get(self.pos + 1) == Some(&'/') {
                    break;
                }
                path.push('.');
                self.advance();
                self.skip_whitespace();
            } else {
                break;
            }
        }

        Ok(path)
    }

    fn parse_string(&mut self) -> Result<String, ParseError> {
        let quote = self.current().ok_or_else(|| self.error("Expected quote"))?;

        // Gérer les strings multi-lignes ''...''
        if quote == '\'' && self.input.get(self.pos + 1) == Some(&'\'') {
            self.advance(); // première '
            self.advance(); // deuxième '

            let mut s = String::new();
            while let Some(c) = self.current() {
                if c == '\'' && self.input.get(self.pos + 1) == Some(&'\'') {
                    self.advance();
                    self.advance();
                    return Ok(s);
                }
                s.push(c);
                self.advance();
            }
            return Err(self.error("Unterminated multi-line string"));
        }

        // String normale
        self.advance();

        let mut s = String::new();
        let mut escaped = false;

        while let Some(c) = self.current() {
            if escaped {
                s.push(match c {
                    'n' => '\n',
                    't' => '\t',
                    'r' => '\r',
                    '\\' => '\\',
                    '"' => '"',
                    '\'' => '\'',
                    _ => c,
                });
                escaped = false;
                self.advance();
            } else if c == '\\' {
                escaped = true;
                self.advance();
            } else if c == quote {
                self.advance();
                return Ok(s);
            } else {
                s.push(c);
                self.advance();
            }
        }
        Err(self.error("Unterminated string"))
    }

    fn parse_number(&mut self) -> Result<NixValue, ParseError> {
        let mut num = String::new();
        let mut is_float = false;

        while let Some(c) = self.current() {
            if c.is_numeric() || c == '.' || c == '-' {
                if c == '.' {
                    is_float = true;
                }
                num.push(c);
                self.advance();
            } else {
                break;
            }
        }

        if is_float {
            num.parse::<f64>()
                .map(NixValue::Float)
                .map_err(|_| self.error("Invalid float"))
        } else {
            num.parse::<i64>()
                .map(NixValue::Int)
                .map_err(|_| self.error("Invalid integer"))
        }
    }

    fn parse_list(&mut self) -> Result<NixValue, ParseError> {
        self.advance(); // '['
        self.skip_whitespace();

        let mut items = Vec::new();
        while self.current() != Some(']') && self.current().is_some() {
            items.push(self.parse_value()?);
            self.skip_whitespace();
        }

        if self.current() == Some(']') {
            self.advance(); // ']'
        }
        Ok(NixValue::List(items))
    }

    fn parse_attrset(&mut self) -> Result<NixValue, ParseError> {
        self.advance(); // '{'
        self.skip_whitespace();

        let mut attrs = HashMap::new();

        while self.current() != Some('}') && self.current().is_some() {
            // Gérer 'inherit'
            if self.peek_string("inherit") {
                for _ in 0..7 {
                    self.advance();
                }
                self.skip_whitespace();

                let mut inherited = Vec::new();
                while self.current() != Some(';') && self.current().is_some() {
                    let id = self.parse_identifier()?;
                    inherited.push(id);
                    self.skip_whitespace();
                }
                if self.current() == Some(';') {
                    self.advance(); // ';'
                }
                self.skip_whitespace();
                continue;
            }

            // Parser la clé (peut être un chemin d'attributs comme "services.udev.extraRules")
            let key = if self.current() == Some('"') {
                self.parse_string()?
            } else {
                self.parse_attribute_path()?
            };

            self.skip_whitespace();

            if self.current() != Some('=') {
                return Err(self.error(&format!("Expected '=' after key '{}', found {:?}", key, self.current())));
            }
            self.advance();
            self.skip_whitespace();

            let value = self.parse_value()?;
            attrs.insert(key, value);

            self.skip_whitespace();
            if self.current() == Some(';') {
                self.advance();
                self.skip_whitespace();
            }
        }

        if self.current() == Some('}') {
            self.advance(); // '}'
        }
        Ok(NixValue::AttrSet(attrs))
    }

    fn parse_function_params(&mut self) -> Result<Vec<String>, ParseError> {
        self.advance(); // '{'
        self.skip_whitespace();

        let mut params = Vec::new();

        while self.current() != Some('}') && self.current().is_some() {
            // Gérer le '...' qui termine les paramètres
            if self.peek_string("...") {
                self.advance();
                self.advance();
                self.advance();
                self.skip_whitespace();

                // Peut avoir une virgule après ...
                if self.current() == Some(',') {
                    self.advance();
                    self.skip_whitespace();
                }

                // Si on trouve '}', on sort
                if self.current() == Some('}') {
                    break;
                }
                continue;
            }

            let param = self.parse_identifier()?;
            params.push(param);
            self.skip_whitespace();

            // Gérer la virgule
            if self.current() == Some(',') {
                self.advance();
                self.skip_whitespace();
            }
        }

        if self.current() == Some('}') {
            self.advance(); // '}'
        }
        Ok(params)
    }

    fn parse_let(&mut self) -> Result<NixValue, ParseError> {
        for _ in 0..3 {
            self.advance();
        } // "let"
        self.skip_whitespace();

        let mut bindings = HashMap::new();

        while !self.peek_string("in") && self.current().is_some() {
            let key = self.parse_identifier()?;
            self.skip_whitespace();

            if self.current() != Some('=') {
                return Err(self.error("Expected '=' in let binding"));
            }
            self.advance();
            self.skip_whitespace();

            let value = self.parse_value()?;
            bindings.insert(key, value);

            self.skip_whitespace();
            if self.current() == Some(';') {
                self.advance();
                self.skip_whitespace();
            }
        }

        for _ in 0..2 {
            self.advance();
        } // "in"
        self.skip_whitespace();

        let body = self.parse_value()?;

        Ok(NixValue::Let(Box::new(NixLet { bindings, body })))
    }

    fn parse_value(&mut self) -> Result<NixValue, ParseError> {
        self.skip_whitespace();

        // Détecter une fonction avec pattern { param1, param2, ... }:
        if self.current() == Some('{') {
            let saved_pos = self.pos;
            let saved_line = self.line;
            let saved_col = self.col;

            // Essayer de parser comme paramètres de fonction
            if let Ok(params) = self.parse_function_params() {
                self.skip_whitespace();

                // Vérifier si c'est suivi de ':' pour confirmer que c'est une fonction
                if self.current() == Some(':') {
                    self.advance(); // ':'
                    self.skip_whitespace();

                    let body = self.parse_value()?;
                    return Ok(NixValue::Function(Box::new(NixFunction { params, body })));
                }
            }

            // Si ce n'est pas une fonction, revenir en arrière et parser comme attrset
            self.pos = saved_pos;
            self.line = saved_line;
            self.col = saved_col;
            return self.parse_attrset();
        }

        match self.current() {
            Some('[') => self.parse_list(),
            Some('"') => Ok(NixValue::String(self.parse_string()?)),
            Some('\'') => {
                // Peut être une string multi-ligne ou une string simple
                if self.input.get(self.pos + 1) == Some(&'\'') {
                    Ok(NixValue::String(self.parse_string()?))
                } else {
                    Ok(NixValue::String(self.parse_string()?))
                }
            }
            Some(c) if c.is_numeric() || c == '-' && self.input.get(self.pos + 1).map_or(false, |ch| ch.is_numeric()) => {
                self.parse_number()
            }
            Some('.') => {
                // Peut être un chemin relatif (./path ou ../../path) ou un accès d'attribut
                // On regarde le caractère suivant
                if let Some(next) = self.input.get(self.pos + 1) {
                    if *next == '/' || *next == '.' {
                        // C'est un chemin relatif
                        let path = self.parse_path()?;
                        return Ok(NixValue::Path(path));
                    }
                }
                // Sinon c'est probablement une erreur ou un cas spécial
                Err(self.error("Unexpected '.' - expected path or attribute access"))
            }
            Some('/') => {
                // C'est un chemin absolu
                let path = self.parse_path()?;
                Ok(NixValue::Path(path))
            }
            Some(_) => {
                if self.peek_string("null") {
                    for _ in 0..4 {
                        self.advance();
                    }
                    Ok(NixValue::Null)
                } else if self.peek_string("true") {
                    for _ in 0..4 {
                        self.advance();
                    }
                    Ok(NixValue::Bool(true))
                } else if self.peek_string("false") {
                    for _ in 0..5 {
                        self.advance();
                    }
                    Ok(NixValue::Bool(false))
                } else if self.peek_string("let") {
                    self.parse_let()
                } else if self.peek_string("import") {
                    for _ in 0..6 {
                        self.advance();
                    }
                    self.skip_whitespace();
                    let path = self.parse_value()?;
                    if let NixValue::String(p) | NixValue::Path(p) = path {
                        Ok(NixValue::Import(p))
                    } else {
                        Err(self.error("Expected string or path after import"))
                    }
                } else {
                    // Parser un identifiant/chemin d'attribut ou une fonction simple
                    let id = self.parse_attribute_path()?;
                    self.skip_whitespace();

                    // Vérifier si c'est une fonction simple: param: body
                    if self.current() == Some(':') {
                        self.advance();
                        self.skip_whitespace();
                        let body = self.parse_value()?;
                        Ok(NixValue::Function(Box::new(NixFunction {
                            params: vec![id],
                            body,
                        })))
                    } else {
                        Ok(NixValue::Variable(id))
                    }
                }
            }
            None => Err(self.error("Unexpected end of input")),
        }
    }

    pub fn parse(&mut self) -> Result<NixValue, ParseError> {
        self.parse_value()
    }
}

// Fonction principale pour parser un fichier
pub fn parse_nix_file<P: AsRef<Path>>(path: P) -> Result<NixValue, ParseError> {
    let content = fs::read_to_string(&path)
        .map_err(|e| ParseError {
            message: format!("Failed to read file: {}", e),
            line: 0,
            col: 0,
            context: format!("File: {:?}", path.as_ref()),
        })?;
    let mut parser = NixParser::new(&content);
    parser.parse()
}

// Fonction pour parser une chaîne Nix
pub fn parse_nix_string(input: &str) -> Result<NixValue, ParseError> {
    let mut parser = NixParser::new(input);
    parser.parse()
}

// Formatteur pour écrire des valeurs Nix
impl fmt::Display for NixValue {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.write_with_indent(f, 0)
    }
}

impl NixValue {
    fn write_with_indent(&self, f: &mut fmt::Formatter, indent: usize) -> fmt::Result {
        let indent_str = "  ".repeat(indent);

        match self {
            NixValue::Null => write!(f, "null"),
            NixValue::Bool(b) => write!(f, "{}", b),
            NixValue::Int(i) => write!(f, "{}", i),
            NixValue::Float(fl) => write!(f, "{}", fl),
            NixValue::String(s) => write!(f, "\"{}\"", s.replace('\"', "\\\"")),
            NixValue::Path(p) => write!(f, "{}", p),
            NixValue::Variable(v) => write!(f, "{}", v),
            NixValue::Import(p) => write!(f, "import {}", p),

            NixValue::List(items) => {
                writeln!(f, "[")?;
                for item in items.iter() {
                    write!(f, "{}  ", indent_str)?;
                    item.write_with_indent(f, indent + 1)?;
                    writeln!(f)?;
                }
                write!(f, "{}]", indent_str)
            }

            NixValue::AttrSet(attrs) => {
                writeln!(f, "{{")?;
                for (key, value) in attrs.iter() {
                    // La clé contient déjà les guillemets si nécessaire (format: fileSystems."/".options)
                    write!(f, "{}  {} = ", indent_str, key)?;
                    value.write_with_indent(f, indent + 1)?;
                    writeln!(f, ";")?;
                }
                write!(f, "{}}}", indent_str)
            }

            NixValue::Let(let_expr) => {
                writeln!(f, "let")?;
                for (key, value) in let_expr.bindings.iter() {
                    write!(f, "{}  {} = ", indent_str, key)?;
                    value.write_with_indent(f, indent + 1)?;
                    writeln!(f, ";")?;
                }
                write!(f, "{}in ", indent_str)?;
                let_expr.body.write_with_indent(f, indent)
            }

            NixValue::Function(func) => {
                if func.params.len() == 1 {
                    // Fonction simple: x: body
                    write!(f, "{}: ", func.params[0])?;
                } else {
                    // Fonction avec pattern: { x, y, ... }:
                    write!(f, "{{ ")?;
                    for (i, param) in func.params.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{}", param)?;
                    }
                    write!(f, ", ... }}: ")?;
                }
                func.body.write_with_indent(f, indent)
            }

            _ => write!(f, "/* non implémenté */"),
        }
    }
}

// Fonction pour écrire un agrégat Nix dans un fichier
pub fn write_nix_file<P: AsRef<Path>>(path: P, value: &NixValue) -> Result<(), ParseError> {
    let content = value.to_string();
    fs::write(path, content).map_err(|e| ParseError {
        message: format!("Failed to write file: {}", e),
        line: 0,
        col: 0,
        context: String::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_attrset() {
        let input = r#"{ name = "test"; version = 1; }"#;
        let result = parse_nix_string(input).unwrap();

        if let NixValue::AttrSet(attrs) = result {
            assert_eq!(attrs.len(), 2);
            assert_eq!(attrs.get("name"), Some(&NixValue::String("test".to_string())));
            assert_eq!(attrs.get("version"), Some(&NixValue::Int(1)));
        } else {
            panic!("Expected AttrSet");
        }
    }

    #[test]
    fn test_parse_list() {
        let input = r#"[1 2 3 "test"]"#;
        let result = parse_nix_string(input).unwrap();

        if let NixValue::List(items) = result {
            assert_eq!(items.len(), 4);
        } else {
            panic!("Expected List");
        }
    }

    #[test]
    fn test_parse_let() {
        let input = r#"let x = 5; y = 10; in { result = x; }"#;
        let result = parse_nix_string(input).unwrap();

        if let NixValue::Let(let_expr) = result {
            assert_eq!(let_expr.bindings.len(), 2);
        } else {
            panic!("Expected Let");
        }
    }

    #[test]
    fn test_parse_function_pattern() {
        let input = r#"{ self, config, inputs, pkgs, pkgs-unstable, ... }: { name = "test"; }"#;
        let result = parse_nix_string(input).unwrap();

        if let NixValue::Function(func) = result {
            assert!(func.params.contains(&"self".to_string()));
            assert!(func.params.contains(&"config".to_string()));
            assert!(func.params.contains(&"pkgs".to_string()));
        } else {
            panic!("Expected Function, got {:?}", result);
        }
    }

    #[test]
    fn test_parse_simple_function() {
        let input = r#"x: x + 1"#;
        let result = parse_nix_string(input).unwrap();

        if let NixValue::Function(func) = result {
            assert_eq!(func.params.len(), 1);
            assert_eq!(func.params[0], "x");
        } else {
            panic!("Expected Function");
        }
    }
}
