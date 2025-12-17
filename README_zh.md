# Shellm

[English](README.md) | 简体中文

一个由 AI 驱动的终端助手。用自然语言描述你的需求，让 AI 为你生成 shell 命令。

## 安装

```bash
cargo build --release
```

## 快速开始

1. 设置你的 OpenAI API 密钥：

```bash
export OPENAI_API_KEY="your-api-key"
```

2. 运行 shellm：

```bash
./target/release/shellm
```

3. 像平常一样使用终端。当你需要帮助时，按 `Ctrl+L` 与 AI 对话。

## 工作原理

1. 按 `Ctrl+L` 进入对话模式
2. 用自然语言输入你的问题
3. AI 会建议一个命令
4. 按 `Ctrl+L` 接受命令，或按 `Ctrl+C` 取消

## 使用示例

```
[LLM chat] 输入您的问题。Ctrl+L 接受命令，Ctrl+C 退出，Ctrl+R 展开/折叠思维链。
你> 找出最近7天修改过的所有python文件
助手> 搜索最近修改的 Python 文件
候选命令: find . -name "*.py" -mtime -7
```

## 配置

Shellm 支持通过环境变量和/或 TOML 配置文件进行配置。

### 环境变量

| 变量 | 说明 |
|------|------|
| `OPENAI_API_KEY` | 你的 API 密钥（必需） |
| `OPENAI_MODEL` | 使用的模型（默认：`gpt-4o-mini`） |
| `OPENAI_BASE_URL` | 自定义 API 基础 URL（默认：`https://api.openai.com/v1`） |
| `SHELLM_CONFIG` | 自定义配置文件路径（可选） |

### 配置文件

在 `~/.config/shellm/config.toml` 创建配置文件来自定义 shellm 的行为：

```bash
mkdir -p ~/.config/shellm
cp config.example.toml ~/.config/shellm/config.toml
```

#### 配置示例

```toml
[llm]
model = "gpt-4o-mini"
# api_key = "sk-..."  # 或者使用 OPENAI_API_KEY 环境变量

[prompt]
# 自定义提示词模板，支持动态变量：
#   {os}    - 操作系统（Linux、Windows、macOS）
#   {arch}  - CPU 架构（x86_64、aarch64、riscv64 等）
#   {shell} - 当前 shell（bash、zsh、fish、powershell、cmd）
#   {lang}  - 偏好语言（zh-CN、en-US 等）
template = """
You are a focused shell copilot on {os} ({arch}) running {shell}.
Please answer in {lang}.
Always respond ONLY with a JSON object:
{"command": "<shell command>", "answer": "brief human-readable note"}.
"""

[shell]
# path = "/bin/zsh" # 可选：手动指定 shell 可执行文件路径

[preference]
language = "zh-CN"  # 或从 LANG 环境变量自动检测
```

### 配置优先级

1. 配置文件设置优先于环境变量
2. 如果没有配置文件，则使用环境变量
3. 默认值作为最后的回退

## 许可证

GPL-3.0