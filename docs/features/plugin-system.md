# LiveAgent 插件系统（plugins v1 规格）

> 状态：**v1 规格**（2026-08-03 固化，范围锁定 provider 注入闭环）
> 日期：2026-08-03

LiveAgent 插件是一个**进程内、完全信任、可注入 provider 协议与前端 UI** 的扩展载体。
v1 覆盖 **provider 注入闭环**：插件注册成一个 provider 协议，在供应商配置里与内置协议
并列，用户可基于它创建多个 provider 实例。插件由一段极简清单 + 一个 ESM 注册入口构成，
经冷启动加载生效。

## 1. 插件与扩展面

LiveAgent 已有三个扩展面，各管一段，不重叠：

| 扩展面 | 载体 | 信任模型 | 运行位置 | 职责 |
|---|---|---|---|---|
| **插件** | `.liveagent-plugin` | 完全信任 | 进程内（WebView） | provider 协议、工具 bundle、UI 挂载、深度定制 |
| **MCP** | `settings.mcp.servers` | 受信任配置 | 子进程/外部 server | 进程外工具协议（stdio/http/sse） |
| **Skills** | `~/.liveagent/skills` | 文档资产 | 进程内（仅提示词/文件） | 提示词资产、progressive disclosure |

**MCP 解决"接入外部工具协议"，Skills 解决"给模型喂知识与工作流资产"，插件解决
"进程内可信代码扩展"**。三者不重叠：插件可以*提供* MCP server 配置、可以*打包* Skills，
但插件的本质能力是注册代码扩展点（provider/tool/ui）。

## 2. 包格式

上传文件扩展名 **`.liveagent-plugin`**，内容是 ZIP。生产包至少包含清单与前端入口：

```text
plugin.json
frontend/entry.js          # 必需。ESM，导出 register()。
frontend/assets/...        # 可选。entry 的相对资源，走同一路径前缀。
```

- 归档路径必须是相对 POSIX 路径，不含 `..`、绝对路径、符号链接、加密项或 ZIP64；
  除必需入口外可携带任意额外文件，宿主原样保留。
- 解压校验大小、CRC 与清单后，内容先写入 staging，再**原子 rename** 到
  `~/.liveagent/plugins/<id>/`（单目录覆盖，无版本目录层级）。
- 安装/更新只落盘，变更在**下一次冷启动**生效，运行中不改变前端与运行时。
- LiveAgent 没有原生二进制插件面，前端 ESM 就是唯一执行载体，不存在平台目录、
  双架构产物与"当前平台缺失即报错"。

安装复用 Skills 已成熟的 stage-then-swap 模式
（`crates/agent-gui/src-tauri/src/services/skills/install.rs`）：新内容先在 `.staging/`
下完整构建，最后用 `fs::rename` 原子入位，读者永远看不到半成品。

## 3. 清单

`plugin.json` 只有五个字段，无 `format_version`、`core_abi`、`targets`、`requires`、
`load_order`、`migrations`：

```json
{
  "id": "my-provider",
  "type": "provider",
  "name": "My Provider",
  "description": "接入示例 provider 协议",
  "version": "0.1.0"
}
```

- `id`、`version` 只含字母、数字、`-`、`_`、`.`。ID 大小写敏感，改变 ID（含大小写
  变化）表示另一个插件，不表示升级。
- `type` 是插件**主类别**枚举，未知值不属于有效包（安装被拒）。**v1 只认 `provider`**，
  `tool`/`ui` 值安装时拒绝（未实现，不预留）：
  - `provider`：参与模型协议数据面。`type` 即协议 id 的来源（见 §4），协议 id = `plugin:<id>`。
- **能力面与 `type` 不要求一一对应**：实际能力由 `entry.js` 导出的 `register(api)` 声明，
  `type` 只用于分组与展示。v1 仅实现 `provider` 能力面，`tool`/`ui` 能力面后补。

## 4. 前端入口与注册 API

`frontend/entry.js` 是普通 ESM，**导出 `register(api)` 函数**——显式注册让核心能拿到
能力声明，比"靠模块副作用自挂载"更可控：

```ts
import type { PluginApi } from "@liveagent/plugin-api";

export function register(api: PluginApi): void {
  api.registerProvider({
    providerId: "my-provider",      // 协议 id 的后缀；协议 id = plugin:<plugin.json.id>
    label: "My Provider",
    models: [
      { id: "deepseek-chat", contextWindow: 65536, maxOutputToken: 8192 },
    ],
    createModel: (params) => ({
      // 返回"一个能用的 model"——核心按 model.api 走现有 streamSimpleByApi 流式管道
      api: "openai-completions",   // 复用现有协议，v1 不写 buildStream
      id: params.modelId,
      baseUrl: params.baseUrl,
      maxTokens: 8192,
      contextWindow: 65536,
    }),
  });
}
```

### 协议 id

插件 provider 的协议 id = **`plugin:<plugin.json.id>`**。`registerProvider.providerId`
字段实际就是插件包 id（协议 id 即 `plugin:<id>`）。一个插件包绑定一个 provider 协议。

- `ProviderId` union 扩展为：内置四协议（`claude_code`/`codex`/`gemini`/`xai`）+ 插件协议
  id（`plugin:<id>`）。
- 冲突策略：`plugin:<id>` 前缀与内置协议天然隔离；两个插件若注册相同 `plugin:<id>`，安装时
  先到先得、后装者拒绝（见 §7）。

### 模型目录

模型列表声明在**插件内部**（`registerProvider.models`），**不进 `plugin.json`**——它跟着
协议适配逻辑走，是运行时注册的一部分。插件协议 tab 用这些 `models` 预填模型选择器；
同一协议创建的多个 provider 实例共享这份初始列表（实例内 `activeModels` 可单独启停）。

### `createModel`：复用现有协议，不写 `buildStream`

v1 插件 provider **复用现有模型协议**，核心按 `createModel` 返回的 `model.api` 交给现有
`streamSimpleByApi` 管道。插件只声明协议 + 构造 model，**不写 `buildStream`**：

- `createModel(params)` 返回 `{ api, id, baseUrl, maxTokens?, contextWindow? }`，
  `api ∈ { anthropic-messages, openai-completions, openai-responses, google-generative-ai }`。
- 核心在 `createModelFromConfig` 的 `plugin:<id>` 分支调用插件注入的 `createModel`，
  产出 `Model` 后走 `streamSimpleByApi`（按 `model.api` 分派到现有流式实现）。
- **插件自管 Model 构造**：contextWindow/maxTokens 等元数据由插件声明，核心不猜。
- `buildStream` 在 v1 不做。仅当未来需要支持 pi-ai 不认识的私有协议时才引入。

### `PluginApi` 是核心对插件暴露的**唯一表面**：

- `registerProvider(...)`：注入 provider adapter（v1 实现）。
- `registerToolBundle(...)`：注入工具 bundle（后补，复用 `BuiltinToolBundle` 形状）。
- `registerUi(...)`：声明 UI 挂载点（后补）。
- `store`：只读写插件自身命名空间（`plugin:<id>:*`），经 Tauri command，见 §6。

## 5. provider 协议

**插件 provider 是一个协议**，与内置四协议（`claude_code`/`codex`/`gemini`/`xai`）并列。
在供应商配置页（`crates/agent-gui/src/pages/settings/ProvidersSection.tsx`）顶部 tab 里，
插件协议作为新增 tab 出现；用户在该 tab 下**创建 provider 实例**（填 baseUrl/apiKey），
一个协议可被多个实例复用。

**provider 分发链路**：`providerId → adapter → stream adapter`。对插件 provider 具体化为
`plugin:<id> → createModel → streamSimpleByApi`：

- 选中模型 `SelectedModel { customProviderId, model }` → 在 `customProviders` 里找到
  provider（`type = plugin:<id>`）。
- `createModelFromConfig` 对 `plugin:<id>` 分支调用插件注入的 `createModel`，产出 `Model`
  后按 `model.api` 走现有流式管道（`streamSimpleByApi`），核心不新增协议分支。
- 模型列表在插件内部（`registerProvider.models`），实例 `activeModels` 可单独启停。

**与内置 provider 的差异**：v1 插件 provider 只做**纯协议流式**，不做 usage 查询 /
原生搜索（这些是内置 provider 的增值能力，后补）。v1 无 HTTP 端点面；`registerToolBundle`
/`registerUi` 后补。

## 6. 生命周期与存储

| 操作 | 语义 |
|---|---|
| 安装 / 更新 | stage-then-swap 原子替换 `~/.liveagent/plugins/<id>/`；同 ID 覆盖，无版本并存、无手动回滚。 |
| 启用 / 禁用 | `~/.liveagent/config.sqlite` 的 `plugins` 表记录启用状态；禁用插件不参与冷启动加载。 |
| 卸载 | 删除插件目录 + 清 `plugin_data` 表中该插件的行（见下）；无 pending/cleanup 机制。 |
| 生效时机 | **下一次冷启动**。运行中不热加载、不热卸载，避免运行时状态不一致。 |
| 失败语义 | 单个插件加载失败（语法错误/异常/资源缺失）→ 仅禁用该插件并告警，主应用继续启动。 |

**插件管理页（v1 范围）**：本地 `.liveagent-plugin` 文件**导入**（校验 + 解压 + staging→rename）、
已装插件**列表**（含启用/禁用开关）、**卸载**。复用 `services/skills/install.rs` 的
stage-then-swap 模式（见 §2）。

**插件数据（`api.store`）落点**：`config.sqlite` 新建 `plugin_data` 表，主键 `(plugin_id, key)`。
插件经 `api.store.get/set` 只读写**自身命名空间**（`plugin:<id>:*`），与其他插件/内置数据隔离。
卸载时按 `plugin_id` 删除该插件全部行。

## 7. 安全与信任

- **完全信任，无沙箱、无签名、无能力白名单**。安装插件等价于把任意前端代码放进 WebView 进程。
- 信任半径：插件能调用 `PluginApi` 表面与 Tauri command；**不**直接持有文件系统、Shell、
  SQLite 的裸权限。高权限能力仍收敛在 Rust 侧（沿用 LiveAgent 现有取舍：高权限能力放 Rust）。
- 工具名冲突：复用 `builtinRegistry.ts` 既有策略——第三方来源（MCP/插件）撞车时
  **先到先得、跳过后来者并告警，绝不 throw 打断整轮**；仅两侧都是可信内置组时才 throw
  （编译期开发 bug）。
- **插件协议 id 冲突**：协议 id 用 `plugin:<id>` 前缀，与内置四协议天然隔离；两个插件若
  注册相同 `plugin:<id>`，**安装时先到先得、后装者拒绝**（不静默跳过——协议 id 是安装面
  的硬冲突，区别于工具名的运行时先到先得）。
- 插件加载失败只影响该插件，不阻塞主应用（见 §6 失败语义）。

## 8. 加载机制

插件 `frontend/entry.js` 经 **Tauri 资产协议动态 `import`** 加载；`register(api)` 时核心把
`api` 注入。实现细节以 Rust/WebView 实际能力为准。

---

## 设计来源

本文规格的格式与取舍承自 Revlm 的包格式 v1（`/Users/realm/revlm/docs/plugin-package-format.md`）。
Revlm 是 C++ 内核，LiveAgent 是 TypeScript/React + Rust/Tauri + Go Gateway；LiveAgent 没有
原生插件面，因此保留了 Revlm 经多轮讨论沉淀出的架构决策（完全信任、极简清单、目录约定即
声明、文件系统为安装真相、冷启动生效、冲突不检测），砍掉了语言绑定产物（原生 `.so`/ABI、
插件自管数据库、热加载）。

---

## 参考

- LiveAgent 工具注册：`crates/agent-gui/src/lib/tools/builtinRegistry.ts`、`docs/features/tools.md`
- LiveAgent Skills 安装（stage-then-swap）：`crates/agent-gui/src-tauri/src/services/skills/install.rs`、`docs/features/skills-and-mcp.md`
- LiveAgent provider 层：`crates/agent-gui/src/lib/providers/llm.ts`、`runtime/*`
- LiveAgent 供应商配置 UI：`crates/agent-gui/src/pages/settings/ProvidersSection.tsx`
