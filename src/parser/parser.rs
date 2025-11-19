use anyhow::{Result, anyhow};
use pest::Parser;
use pest_derive::Parser;
use std::collections::HashMap;
use tracing::debug;

use crate::ast::*;

#[derive(Parser)]
#[grammar = "parser/sprout.pest"]
pub struct SproutParser;

pub fn parse_manifest(input: &str) -> Result<SproutManifest> {
    debug!("Starting to parse manifest");

    let pairs =
        SproutParser::parse(Rule::manifest, input).map_err(|e| anyhow!("Parse error: {}", e))?;

    let mut modules = Vec::new();
    let mut environments = None;

    for pair in pairs {
        match pair.as_rule() {
            Rule::manifest => {
                debug!("Processing manifest rule");
                for inner_pair in pair.into_inner() {
                    match inner_pair.as_rule() {
                        Rule::statement => {
                            debug!("Found statement, processing inner content");
                            // Handle the statement rule by processing its inner content
                            for statement_inner in inner_pair.into_inner() {
                                match statement_inner.as_rule() {
                                    Rule::module_block => {
                                        debug!("Found package block inside statement");
                                        modules.push(parse_module_block(statement_inner)?);
                                    }
                                    Rule::environments_block => {
                                        debug!("Found environments block inside statement");
                                        environments =
                                            Some(parse_environments_block(statement_inner)?);
                                    }
                                    _ => {
                                        debug!(
                                            "Unexpected rule inside statement: {:?}",
                                            statement_inner.as_rule()
                                        );
                                    }
                                }
                            }
                        }
                        Rule::module_block => {
                            debug!("Found package block");
                            modules.push(parse_module_block(inner_pair)?);
                        }
                        Rule::environments_block => {
                            debug!("Found environments block");
                            environments = Some(parse_environments_block(inner_pair)?);
                        }
                        Rule::EOI => {
                            debug!("Reached end of input");
                            break;
                        }
                        _ => {
                            debug!("Unexpected rule: {:?}", inner_pair.as_rule());
                        }
                    }
                }
            }
            _ => {
                debug!("Unexpected top-level rule: {:?}", pair.as_rule());
            }
        }
    }

    debug!("Parsed {} modules", modules.len());
    modules.sort_by_key(|p| p.id());
    Ok(SproutManifest {
        modules,
        environments,
    })
}

fn parse_module_block(pair: pest::iterators::Pair<Rule>) -> Result<ModuleBlock> {
    let mut inner = pair.into_inner();

    // Parse package ID (name)
    let module_id = inner.next().ok_or_else(|| anyhow!("Missing package ID"))?;
    let name = module_id.as_str().to_string();

    let mut depends_on = Vec::new();
    let mut exports = Vec::new();
    let mut fetch = None;
    let mut build = None;

    let mut update = None;

    // Parse package fields
    for field in inner {
        debug!("Processing package field: {:?}", field.as_rule());
        match field.as_rule() {
            Rule::module_field => {
                // Handle the module_field wrapper by processing its inner content
                let inner_field = field.into_inner().next().unwrap();
                debug!("Inner package field: {:?}", inner_field.as_rule());
                match inner_field.as_rule() {
                    Rule::depends_on_field => {
                        debug!("Parsing depends_on field");
                        depends_on = parse_array(inner_field.into_inner().next().unwrap())?;
                    }
                    Rule::exports_field => {
                        debug!("Parsing exports field");
                        exports = parse_exports_map(inner_field.into_inner().next().unwrap())?;
                    }
                    Rule::fetch_block => {
                        debug!("Parsing fetch block");
                        fetch = Some(parse_fetch_block(inner_field)?);
                    }
                    Rule::build_block => {
                        debug!("Parsing build block");
                        build = Some(parse_script_block(inner_field)?);
                    }

                    Rule::update_block => {
                        debug!("Parsing update block");
                        update = Some(parse_script_block(inner_field)?);
                    }
                    _ => {
                        debug!(
                            "Unknown inner package field rule: {:?}",
                            inner_field.as_rule()
                        );
                    }
                }
            }
            Rule::depends_on_field => {
                debug!("Parsing depends_on field");
                depends_on = parse_array(field.into_inner().next().unwrap())?;
            }
            Rule::exports_field => {
                debug!("Parsing exports field");
                exports = parse_exports_map(field.into_inner().next().unwrap())?;
            }
            Rule::fetch_block => {
                debug!("Parsing fetch block");
                fetch = Some(parse_fetch_block(field)?);
            }
            Rule::build_block => {
                debug!("Parsing build block");
                build = Some(parse_script_block(field)?);
            }

            Rule::update_block => {
                debug!("Parsing update block");
                update = Some(parse_script_block(field)?);
            }
            _ => {
                debug!("Unknown package field rule: {:?}", field.as_rule());
            }
        }
    }

    Ok(ModuleBlock {
        name,
        depends_on,
        exports,
        fetch,
        build,
        update,
    })
}

fn parse_fetch_block(pair: pest::iterators::Pair<Rule>) -> Result<FetchBlock> {
    let mut spec = None;
    let mut output = None;

    for field in pair.into_inner() {
        match field.as_rule() {
            Rule::fetch_field => {
                let inner = field.into_inner().next().ok_or_else(|| anyhow!("Empty fetch field"))?;
                match inner.as_rule() {
                    Rule::fetch_output_field => {
                        let value = inner.into_inner().next().ok_or_else(|| anyhow!("Missing output value"))?;
                        output = Some(parse_value(value)?);
                    }
                    Rule::fetch_spec => {
                        spec = Some(parse_fetch_spec(inner)?);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    Ok(FetchBlock {
        spec: spec.ok_or_else(|| anyhow!("Missing fetch spec"))?,
        output,
    })
}

fn parse_fetch_spec(fetch_spec: pest::iterators::Pair<Rule>) -> Result<FetchSpec> {
    let inner_spec = fetch_spec
        .into_inner()
        .next()
        .ok_or_else(|| anyhow!("Missing inner fetch spec"))?;
    let inner_rule = inner_spec.as_rule();
    debug!("Inner fetch spec rule: {:?}", inner_rule);

    match inner_rule {
        Rule::git_spec => {
            let mut url = None;
            let mut ref_ = None;
            let mut recursive = false;

            for field in inner_spec.into_inner() {
                if field.as_rule() == Rule::git_field {
                    let inner_field = field.into_inner().next().unwrap();
                    match inner_field.as_rule() {
                        Rule::git_url_field => {
                            let mut parts = inner_field.into_inner();
                            let value = parts.next().unwrap();
                            url = Some(parse_value(value)?);
                        }
                        Rule::git_ref_field => {
                            let mut parts = inner_field.into_inner();
                            let value = parts.next().unwrap();
                            ref_ = Some(parse_value(value)?);
                        }
                        Rule::git_recursive_field => {
                            let mut parts = inner_field.into_inner();
                            let value = parts.next().unwrap();
                            recursive = parse_value(value)? == "true";
                        }
                        _ => {}
                    }
                }
            }

            Ok(FetchSpec::Git(GitSpec {
                url: url.ok_or_else(|| anyhow!("Git spec missing url"))?,
                ref_,
                recursive,
            }))
        }
        Rule::http_spec => {
            let mut url = None;
            let mut sha256 = None;

            for field in inner_spec.into_inner() {
                if field.as_rule() == Rule::http_field {
                    let inner_field = field.into_inner().next().unwrap();
                    match inner_field.as_rule() {
                        Rule::http_url_field => {
                            let mut parts = inner_field.into_inner();
                            let value = parts.next().unwrap();
                            url = Some(parse_value(value)?);
                        }
                        Rule::http_sha256_field => {
                            let mut parts = inner_field.into_inner();
                            let value = parts.next().unwrap();
                            sha256 = Some(parse_value(value)?);
                        }
                        _ => {}
                    }
                }
            }

            Ok(FetchSpec::Http(HttpSpec {
                url: url.ok_or_else(|| anyhow!("HTTP spec missing url"))?,
                sha256,
            }))
        }
        Rule::local_spec => {
            let mut path = None;

            for field in inner_spec.into_inner() {
                if field.as_rule() == Rule::local_field {
                    let value = field.into_inner().next().unwrap();
                    path = Some(parse_value(value)?);
                }
            }

            Ok(FetchSpec::Local(LocalSpec {
                path: path.ok_or_else(|| anyhow!("Local spec missing path"))?,
            }))
        }
        _ => {
            Err(anyhow!(
                "Unsupported inner fetch spec type: {:?}",
                inner_rule
            ))
        }
    }
}

fn parse_script_block(pair: pest::iterators::Pair<Rule>) -> Result<ScriptBlock> {
    let mut env = Vec::new();
    let mut commands = Vec::new();

    debug!("Parsing script block, rule: {:?}", pair.as_rule());
    for inner in pair.into_inner() {
        debug!("Script block inner rule: {:?}", inner.as_rule());
        match inner.as_rule() {
            Rule::env_block => {
                debug!("Found env_block");
                for env_entry in inner.into_inner() {
                    let mut entry_inner = env_entry.into_inner();
                    let key = entry_inner.next().unwrap().as_str().to_string();
                    let value = parse_string(entry_inner.next().unwrap())?;
                    debug!("Env entry: {} = {}", key, value);
                    env.push((key, value));
                }
            }
            Rule::script_content => {
                debug!("Found script_content");
                // script_content contains the command lines
                for content_inner in inner.into_inner() {
                    debug!("Script content inner rule: {:?}", content_inner.as_rule());
                    match content_inner.as_rule() {
                        Rule::command_line => {
                            let cmd = content_inner.as_str().trim();
                            if !cmd.is_empty() {
                                commands.push(cmd.to_string());
                            }
                        }
                        Rule::env_block => {
                            debug!("Found env_block inside script_content");
                            for env_entry in content_inner.into_inner() {
                                let mut entry_inner = env_entry.into_inner();
                                let key = entry_inner.next().unwrap().as_str().to_string();
                                let value = parse_string(entry_inner.next().unwrap())?;
                                debug!("Env entry: {} = {}", key, value);
                                env.push((key, value));
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    Ok(ScriptBlock { env, commands })
}

fn parse_environments_block(pair: pest::iterators::Pair<Rule>) -> Result<EnvironmentsBlock> {
    let mut environments = HashMap::new();

    for entry in pair.into_inner() {
        let mut entry_inner = entry.into_inner();
        let name = entry_inner.next().unwrap().as_str().to_string();
        let array = parse_array(entry_inner.next().unwrap())?;
        environments.insert(name, array);
    }

    Ok(EnvironmentsBlock { environments })
}

fn parse_value(pair: pest::iterators::Pair<Rule>) -> Result<String> {
    match pair.as_rule() {
        Rule::string => parse_string(pair),
        Rule::unquoted_value => Ok(pair.as_str().to_string()),
        Rule::value => {
            // Handle the value rule by processing its inner content
            let inner = pair
                .into_inner()
                .next()
                .ok_or_else(|| anyhow!("Empty value"))?;
            parse_value(inner)
        }
        _ => Err(anyhow!("Unexpected value type: {:?}", pair.as_rule())),
    }
}

fn parse_array(pair: pest::iterators::Pair<Rule>) -> Result<Vec<String>> {
    let mut result = Vec::new();
    for item in pair.into_inner() {
        if item.as_rule() == Rule::value {
            result.push(parse_value(item)?);
        }
    }
    Ok(result)
}

fn parse_exports_map(pair: pest::iterators::Pair<Rule>) -> Result<Vec<(String, String)>> {
    let mut result = Vec::new();

    for entry in pair.into_inner() {
        let mut entry_inner = entry.into_inner();
        let key = entry_inner.next().unwrap().as_str().to_string();
        let value = entry_inner.next().unwrap();

        // exports_entry always has a string value
        let parsed_value = parse_string(value)?;
        result.push((key, parsed_value));
    }

    Ok(result)
}

fn parse_string(pair: pest::iterators::Pair<Rule>) -> Result<String> {
    // Since string is atomic (@), we get the full string including quotes
    let s = pair.as_str();

    // Remove surrounding quotes
    if s.len() < 2 || !s.starts_with('"') || !s.ends_with('"') {
        return Err(anyhow!("Invalid string format: {}", s));
    }

    let inner = &s[1..s.len() - 1]; // Remove quotes

    // Handle escape sequences
    let result = inner
        .replace("\\\"", "\"")
        .replace("\\\\", "\\")
        .replace("\\n", "\n")
        .replace("\\r", "\r")
        .replace("\\t", "\t");

    Ok(result)
}
