use std::env;
use std::fs;
use std::process::exit;

struct Item { name: String, price: f64, tags: String }

fn parse(src: &str) -> Result<Vec<Item>, String> {
    let mut items = Vec::new();
    let mut cur: Option<Item> = None;
    for (n, raw) in src.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') { continue; }
        if line == "[[item]]" {
            if let Some(it) = cur.take() { items.push(it); }
            cur = Some(Item { name: String::new(), price: 0.0, tags: String::new() });
            continue;
        }
        let (k, v) = line.split_once('=').ok_or(format!("line {}: expected key = value", n + 1))?;
        let it = cur.as_mut().ok_or(format!("line {}: key outside [[item]]", n + 1))?;
        let v = v.trim();
        match k.trim() {
            "name" => it.name = v.trim_matches('"').to_string(),
            "price" => it.price = v.parse().map_err(|e| format!("line {}: bad price: {}", n + 1, e))?,
            "tags" => it.tags = v.trim_matches('"').to_string(),
            other => return Err(format!("line {}: unknown key '{}'", n + 1, other)),
        }
    }
    if let Some(it) = cur.take() { items.push(it); }
    Ok(items)
}

fn check(items: &[Item]) -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();
    if items.is_empty() { return Err("menu has no items".into()); }
    for it in items {
        if it.name.is_empty() { return Err("item with empty name".into()); }
        if it.price <= 0.0 { return Err(format!("item '{}': price must be positive", it.name)); }
        if !seen.insert(it.name.clone()) { return Err(format!("duplicate item '{}'", it.name)); }
    }
    Ok(())
}

fn to_json(items: &[Item]) -> String {
    let rows: Vec<String> = items.iter().map(|i| format!(
        "  {{\"name\": \"{}\", \"price\": {:.2}, \"currency\": \"USD\", \"tags\": \"{}\"}}", i.name, i.price, i.tags
    )).collect();
    format!("[\n{}\n]\n", rows.join(",\n"))
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let r = match args.as_slice() {
        [flag, input] if flag == "--check" => fs::read_to_string(input).map_err(|e| e.to_string())
            .and_then(|s| parse(&s))
            .and_then(|items| { check(&items)?; println!("menu ok: {} items", items.len()); Ok(()) }),
        [input, output] => fs::read_to_string(input).map_err(|e| e.to_string())
            .and_then(|s| parse(&s))
            .and_then(|items| { check(&items)?; fs::write(output, to_json(&items)).map_err(|e| e.to_string()) }),
        _ => Err("usage: menugen <menu.toml> <menu.json> | menugen --check <menu.toml>".into()),
    };
    if let Err(e) = r { eprintln!("menugen: {}", e); exit(1); }
}
