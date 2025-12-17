use std::collections::HashMap;
use std::env;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::Deserialize;

const DEFAULT_PROMPT_TEMPLATE: &str = r#"You are a focused shell copilot on {os} ({arch}) running {shell}.
Please answer in {lang}.
Always respond with a markdown code block containing a JSON object:
```json
{"command": "<shell command>", "answer": "brief human-readable note"}
```
Prefer safe defaults; if unsure ask via answer."#;

#[derive(Debug, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub llm: LlmConfig,
    #[serde(default)]
    pub prompt: PromptConfig,
    #[serde(default)]
    pub shell: ShellConfig,
    #[serde(default)]
    pub preference: PreferenceConfig,
}

#[derive(Debug, Deserialize, Default)]
pub struct LlmConfig {
    pub api_key: Option<String>,
    pub model: Option<String>,
    pub base_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PromptConfig {
    #[serde(default = "default_prompt_template")]
    pub template: String,
}

impl Default for PromptConfig {
    fn default() -> Self {
        Self {
            template: DEFAULT_PROMPT_TEMPLATE.to_string(),
        }
    }
}

fn default_prompt_template() -> String {
    DEFAULT_PROMPT_TEMPLATE.to_string()
}

#[derive(Debug, Deserialize, Default)]
pub struct ShellConfig {
    /// Shell executable path. If not set, auto-detect based on OS.
    pub path: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct PreferenceConfig {
    pub language: Option<String>,
}

#[derive(Debug)]
pub struct SystemInfo {
    pub os: String,
    pub arch: String,
    pub shell: String,
    pub lang: String,
}

impl SystemInfo {
    pub fn collect(preference_lang: Option<&str>) -> Self {
        Self {
            os: Self::detect_os(),
            arch: Self::detect_arch(),
            shell: Self::detect_shell(),
            lang: Self::detect_lang(preference_lang),
        }
    }

    fn detect_os() -> String {
        if cfg!(target_os = "windows") {
            "Windows".to_string()
        } else if cfg!(target_os = "macos") {
            "macOS".to_string()
        } else if cfg!(target_os = "linux") {
            "Linux".to_string()
        } else {
            env::consts::OS.to_string()
        }
    }

    fn detect_arch() -> String {
        env::consts::ARCH.to_string()
    }

    fn detect_shell() -> String {
        // Prefer SHELL environment variable
        if let Ok(shell_path) = env::var("SHELL") {
            if let Some(name) = shell_path.rsplit('/').next() {
                return name.to_string();
            }
        }
        // Special handling for Windows
        if cfg!(target_os = "windows") {
            if env::var("PSModulePath").is_ok() {
                return "powershell".to_string();
            }
            return "cmd".to_string();
        }
        "unknown".to_string()
    }

    fn detect_lang(preference: Option<&str>) -> String {
        // Prefer the configured preference
        if let Some(lang) = preference {
            return lang.to_string();
        }
        // Infer from LANG environment variable
        if let Ok(lang) = env::var("LANG") {
            // Extract language code, e.g. "zh_CN.UTF-8" -> "zh-CN"
            let lang_code = lang.split('.').next().unwrap_or(&lang);
            return lang_code.replace('_', "-");
        }
        "en-US".to_string()
    }

    pub fn to_vars(&self) -> HashMap<&str, &str> {
        let mut vars = HashMap::new();
        vars.insert("os", self.os.as_str());
        vars.insert("arch", self.arch.as_str());
        vars.insert("shell", self.shell.as_str());
        vars.insert("lang", self.lang.as_str());
        vars
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        // 1. Check path specified by environment variable
        if let Ok(path) = env::var("SHELLM_CONFIG") {
            let path = PathBuf::from(path);
            if path.exists() {
                return Self::load_from_file(&path);
            }
        }

        // 2. Check XDG config directory
        if let Some(config_dir) = dirs::config_dir() {
            let path = config_dir.join("shellm").join("config.toml");
            if path.exists() {
                return Self::load_from_file(&path);
            }
        }

        // 3. Fall back to default configuration
        Ok(Self::default())
    }

    fn load_from_file(path: &PathBuf) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;
        toml::from_str(&content)
            .with_context(|| format!("Failed to parse config file: {}", path.display()))
    }
}

pub fn render_prompt(template: &str, vars: &HashMap<&str, &str>) -> String {
    let mut result = template.to_string();
    for (key, value) in vars {
        let placeholder = format!("{{{}}}", key);
        result = result.replace(&placeholder, value);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_prompt() {
        let mut vars = HashMap::new();
        vars.insert("os", "Linux");
        vars.insert("arch", "x86_64");
        vars.insert("shell", "bash");
        vars.insert("lang", "zh-CN");

        let template = "OS: {os}, Arch: {arch}, Shell: {shell}, Lang: {lang}";
        let result = render_prompt(template, &vars);
        assert_eq!(result, "OS: Linux, Arch: x86_64, Shell: bash, Lang: zh-CN");
    }

    #[test]
    fn test_render_prompt_missing_var() {
        let vars = HashMap::new();
        let template = "Hello {name}!";
        let result = render_prompt(template, &vars);
        // Unreplaced variables are left as-is
        assert_eq!(result, "Hello {name}!");
    }

    #[test]
    fn test_system_info_collect() {
        let info = SystemInfo::collect(Some("zh-CN"));
        assert!(!info.os.is_empty());
        assert!(!info.arch.is_empty());
        assert_eq!(info.lang, "zh-CN");
    }
}
