//! Operator signatures supplement the PTE's operator names. No runtime Python
//! installation or network access is needed; see THIRD_PARTY.md for provenance.
use std::{collections::BTreeMap, sync::LazyLock};

#[derive(Debug)]
pub struct Parameter {
    pub name: String,
    pub datatype: String,
    pub output: bool,
    alias: Option<String>,
    keyword: bool,
}
#[derive(Debug)]
pub struct Schema {
    pub arguments: Vec<Parameter>,
    pub returns: Vec<Parameter>,
}

/// Split only at separators outside type aliases, lists and quoted defaults.
fn split(text: &str) -> Vec<&str> {
    let mut depth = 0;
    let mut quote = None;
    let mut escape = false;
    let mut start = 0;
    let mut parts = Vec::new();
    for (i, c) in text.char_indices() {
        if escape {
            escape = false;
        } else if quote.is_some() && c == '\\' {
            escape = true;
        } else if quote == Some(c) {
            quote = None;
        } else if quote.is_none() {
            match c {
                '\'' | '"' => quote = Some(c),
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth -= 1,
                ',' if depth == 0 => {
                    parts.push(text[start..i].trim());
                    start = i + 1;
                }
                _ => {}
            }
        }
    }
    if start < text.len() {
        parts.push(text[start..].trim());
    }
    parts
}

fn parameter(text: &str, index: usize, keyword: bool) -> Option<Parameter> {
    let text = text.split('=').next()?.trim();
    let mut depth = 0;
    let separator = text.char_indices().find_map(|(i, c)| {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            ' ' if depth == 0 => return Some(i),
            _ => {}
        }
        None
    });
    let (datatype, name) = match separator {
        Some(i) => (&text[..i], text[i..].trim().to_string()),
        None => (text, format!("output {index}")),
    };
    let alias = if datatype.starts_with("Tensor") {
        datatype.split_once('(').and_then(|(_, tail)| {
            let alias = tail
                .split(')')
                .next()?
                .split(" -> ")
                .next()?
                .trim_end_matches('!');
            (!alias.is_empty() && !alias.contains(['|', '*'])).then(|| alias.to_string())
        })
    } else {
        None
    };
    Some(Parameter {
        name,
        datatype: datatype.into(),
        output: false,
        alias,
        keyword,
    })
}

fn parse(signature: &str) -> Option<(&str, Schema)> {
    let (name, tail) = signature.split_once('(')?;
    let mut depth = 1;
    let end = tail.char_indices().find_map(|(i, c)| {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
        (depth == 0).then_some(i)
    })?;
    let result = tail[end + 1..].trim().strip_prefix("->")?.trim();
    let result = result
        .strip_prefix('(')
        .and_then(|r| r.strip_suffix(')'))
        .unwrap_or(result);
    let returns = split(result)
        .into_iter()
        .enumerate()
        .map(|(i, p)| parameter(p, i, false))
        .collect::<Option<Vec<_>>>()?;
    let mut keyword = false;
    let mut arguments = Vec::new();
    for part in split(&tail[..end]) {
        if part == "*" {
            keyword = true;
            continue;
        }
        let mut param = parameter(part, arguments.len(), keyword)?;
        // Keyword write arguments aliased by a return are output buffers.
        // In-place positional self arguments remain inputs as well as returns.
        param.output = param.keyword
            && param.datatype.contains('!')
            && param
                .alias
                .as_ref()
                .is_some_and(|a| returns.iter().any(|r| r.alias.as_ref() == Some(a)));
        arguments.push(param);
    }
    Some((name, Schema { arguments, returns }))
}

static SCHEMAS: LazyLock<BTreeMap<&'static str, Schema>> = LazyLock::new(|| {
    include_str!("generated/operator-schemas.txt")
        .lines()
        .filter_map(parse)
        .collect()
});

pub fn lookup(name: &str, argument_count: usize) -> Option<&'static Schema> {
    SCHEMAS
        .get(name)
        .filter(|s| s.arguments.len() + s.returns.len() == argument_count)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bundled_signatures_parse_and_out_aliases_are_identified() {
        for line in include_str!("generated/operator-schemas.txt").lines() {
            assert!(parse(line).is_some(), "Invalid signature: {line}");
        }
        let conv = lookup("aten::convolution.out", 11).unwrap();
        assert_eq!(conv.arguments[1].name, "weight");
        assert!(conv.arguments[9].output);
        assert!(!conv.arguments[0].output);
        let inplace = lookup("aten::relu_", 2).unwrap();
        assert!(!inplace.arguments[0].output);
        assert!(lookup("aten::convolution.out", 10).is_none());
        let (_, defaults) = parse("test(Tensor x, int[2] stride=[1, 2], str label='a,b', *, Tensor(a!) out) -> Tensor(a!)").unwrap();
        assert_eq!(defaults.arguments.len(), 4);
        assert!(defaults.arguments[3].output);
    }
}
