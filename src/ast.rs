use std::collections::{HashMap, HashSet};

/// Top-level AST node for the manifest
#[derive(Debug, Clone, PartialEq)]
pub struct SproutManifest {
    pub modules: Vec<ModuleBlock>,
    pub environments: Option<EnvironmentsBlock>,
}

impl SproutManifest {
    /// Get all dependencies of a package in topological order (dependencies first)
    pub fn get_all_dependencies(&self, module_id: &str) -> Vec<String> {
        let mut result = Vec::new();
        let mut visited = HashSet::new();
        self.visit_dependencies(module_id, &mut visited, &mut result);
        result
    }

    fn visit_dependencies(&self, module_id: &str, visited: &mut HashSet<String>, result: &mut Vec<String>) {
        if visited.contains(module_id) {
            return;
        }

        if let Some(pkg) = self.modules.iter().find(|p| p.id() == module_id) {
            for dep in &pkg.depends_on {
                self.visit_dependencies(dep, visited, result);
            }
            visited.insert(module_id.to_string());
            result.push(module_id.to_string());
        }
    }
}

/// Package block: package name { ... }
#[derive(Debug, Clone, PartialEq)]
pub struct ModuleBlock {
    pub name: String,
    pub depends_on: Vec<String>,
    pub exports: Vec<(String, String)>,
    pub fetch: Option<FetchBlock>,
    pub build: Option<ScriptBlock>,
    pub update: Option<ScriptBlock>,
}

impl ModuleBlock {
    pub fn id(&self) -> String {
        self.name.clone()
    }
}

/// Fetch block with different source types
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FetchBlock {
    pub spec: FetchSpec,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FetchSpec {
    Git(GitSpec),
    Http(HttpSpec),
    Local(LocalSpec),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GitSpec {
    pub url: String,
    pub ref_: Option<String>, // Using ref_ since ref is a Rust keyword
    pub recursive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HttpSpec {
    pub url: String,
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LocalSpec {
    pub path: String,
}

/// Script block with optional environment and commands
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ScriptBlock {
    pub env: Vec<(String, String)>,
    pub commands: Vec<String>,
}

/// Environments block
#[derive(Debug, Clone, PartialEq)]
pub struct EnvironmentsBlock {
    pub environments: HashMap<String, Vec<String>>,
}

/// Tokens for the lexer
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum Token {
    // Keywords
    Package,
    Environments,
    DependsOn,
    Exports,
    Fetch,
    Build,
    Install,
    Update,
    Env,

    // Fetch types
    Git,
    Tar,
    Zip,
    Gz,
    Http,
    Local,

    // Literals
    Identifier(String),
    String(String),
    Number(u32),

    // Symbols
    LeftBrace,
    RightBrace,
    LeftBracket,
    RightBracket,
    Equals,
    Comma,
    At,

    // Special
    Newline,
    Eof,
}

impl Token {
}

use std::fmt;

pub trait PrettyPrint {
    fn pretty_print(&self) -> String;
}

impl PrettyPrint for SproutManifest {
    fn pretty_print(&self) -> String {
        let mut output = String::new();
        
        let mut modules = self.modules.clone();
        modules.sort_by_key(|p| p.id());
        
        for module in &modules {
            output.push_str(&module.pretty_print());
        }
        
        if let Some(environments) = &self.environments {
            output.push_str(&environments.pretty_print());
        }
        
        output
    }
}

impl PrettyPrint for ModuleBlock {
    fn pretty_print(&self) -> String {
        let mut output = format!("module {} {{\n", self.name);
        
        output.push_str("    depends_on = [");
        for (i, dep) in self.depends_on.iter().enumerate() {
            if i > 0 {
                output.push_str(", ");
            }
            output.push_str(dep);
        }
        output.push_str("]\n");
        
        if !self.exports.is_empty() {
            output.push_str("    exports = {\n");
            for (key, value) in &self.exports {
                output.push_str(&format!("        {} = \"{}\"\n", key, value));
            }
            output.push_str("    }\n");
        }
        
        if let Some(fetch) = &self.fetch {
            output.push_str("    fetch {\n");
            output.push_str(&fetch.spec.pretty_print());
            output.push_str("    }\n");
        }
        
        if let Some(build) = &self.build {
            output.push_str("    build {\n");
            output.push_str(&build.pretty_print());
            output.push_str("    }\n");
        }
        
        if let Some(update) = &self.update {
            output.push_str("    update {\n");
            output.push_str(&update.pretty_print());
            output.push_str("    }\n");
        }
        
        output.push_str("}\n\n");
        output
    }
}

impl PrettyPrint for EnvironmentsBlock {
    fn pretty_print(&self) -> String {
        let mut output = String::from("environments {\n");
        
        let mut envs: Vec<_> = self.environments.iter().collect();
        envs.sort_by_key(|(k, _)| *k);
        
        for (name, modules) in envs {
            output.push_str(&format!("    {} = [", name));
            for (i, package) in modules.iter().enumerate() {
                if i > 0 {
                    output.push_str(", ");
                }
                output.push_str(package);
            }
            output.push_str("]\n\n");
        }
        
        output.push_str("}\n");
        output
    }
}

impl PrettyPrint for FetchSpec {
    fn pretty_print(&self) -> String {
        match self {
            FetchSpec::Git(git) => {
                let mut output = String::from("        git = {\n");
                output.push_str(&format!("            url = {}\n", git.url));
                if let Some(ref_) = &git.ref_ {
                    output.push_str(&format!("            ref = {}\n", ref_));
                }
                if git.recursive {
                    output.push_str("            recursive = true\n");
                }
                output.push_str("        }\n");
                output
            }
            FetchSpec::Http(http) => {
                let mut output = String::from("        http = {\n");
                output.push_str(&format!("            url = {}\n", http.url));
                if let Some(sha256) = &http.sha256 {
                    output.push_str(&format!("            sha256 = {}\n", sha256));
                }
                output.push_str("        }\n");
                output
            }
            FetchSpec::Local(local) => {
                format!("        local = {{\n            path = \"{}\"\n        }}\n", local.path)
            }
        }
    }
}

impl PrettyPrint for ScriptBlock {
    fn pretty_print(&self) -> String {
        let mut output = String::new();
        
        if !self.env.is_empty() {
            output.push_str("        env {\n");
            let mut env_vars: Vec<_> = self.env.iter().collect();
            env_vars.sort_by_key(|(k, _)| k.clone());
            
            for (key, value) in env_vars {
                output.push_str(&format!("            {} = \"{}\"\n", key, value));
            }
            output.push_str("        }\n");
        }
        
        for command in &self.commands {
            output.push_str(&format!("        {}\n", command));
        }
        
        output
    }
}

impl fmt::Display for FetchSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FetchSpec::Git(git) => write!(f, "{}", git),
            FetchSpec::Http(http) => write!(f, "{}", http),
            FetchSpec::Local(local) => write!(f, "{}", local),
        }
    }
}

impl fmt::Display for GitSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ref_str = self.ref_.as_deref().unwrap_or("");
        write!(f, "Git{{url:{},ref:{},recursive:{}}}", self.url, ref_str, self.recursive)
    }
}

impl fmt::Display for HttpSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sha256_str = self.sha256.as_deref().unwrap_or("");
        write!(f, "Http{{url:{},sha256:{}}}", self.url, sha256_str)
    }
}

impl fmt::Display for LocalSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Local{{path:{}}}", self.path)
    }
}

impl fmt::Display for ScriptBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ScriptBlock{{env:[")?;
        for (i, (key, value)) in self.env.iter().enumerate() {
            if i > 0 {
                write!(f, ",")?;
            }
            write!(f, "{}={}", key, value)?;
        }
        write!(f, "],commands:[")?;
        for (i, cmd) in self.commands.iter().enumerate() {
            if i > 0 {
                write!(f, ",")?;
            }
            write!(f, "{}", cmd)?;
        }
        write!(f, "]}}")
    }
}
