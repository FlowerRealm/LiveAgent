# LiveAgent 插件系统设计（plugins v1 · 迁移自 Revlm 包格式 v1）

> 状态：**v1 规格**（2026-08-03 经 grill 固化，范围锁定 provider 注入闭环）
> 日期：2026-08-03
> 上游参照：Revlm plugins v3（包格式 v1），见
> `/Users/realm/revlm/docs/plugin-package-format.md`、`/Users/realm/revlm/CONTEXT.md` 与
> 5 份 ADR（`docs/adr/0001-0005`）。
>
> 本文是**迁移设计**，不是 Revlm 文档的复制。Revlm 是 C++ 内核，LiveAgent 是
> TypeScript/React + Rust/Tauri + Go Gateway；直接照搬会把一套为 ELF 符号替换设计的
> 机制硬塞进一个没有原生插件面的项目。迁移的对象是 Revlm 经过多轮 grill 沉淀出的
> **架构决策**，处理对象是它的**语言绑定机制**：保留的保留、翻译的翻译、砍掉的砍掉。

---

## 0.5 本文是 v1 规格（grill 已固化）

2026-08-03 经 grill 逐项收敛，本文从"设计草案"固化为可实施的 **v1 规格**。已定决策（下文各节详述）：
- **范围**：provider 注入闭环 + 插件管理页 CRUD。tool/ui 能力面、`api.proxy`、模型选择器合并 UI、usage/nativeWebSearch、`buildStream` 均**不在 v1**。
- **插件 = 内部协议**：插件 provider 是一个协议（`plugin:<id>`），在供应商配置里与 OpenAI/Claude/Gemini/Grok **并列**；一个协议可被多个 provider 实例复用。
- **无原生二进制面**：Tauri 不提供运行期动态加载编译插件（无稳定 ABI），插件是**纯前端 ESM + 核心内建库**，主动生命周期能力经 `api` 由核心提供。
- **复用现有协议**：插件 `createModel` 返回一个 `api ∈ {anthropic-messages, openai-completions, openai-responses, google-generative-ai}` 的 model，核心走现有 `streamSimpleByApi`。**不写 `buildStream`**。

---

## 0. 核心判断

```text
【核心判断】
✅ 值得做。LiveAgent 已有 MCP 和 Skills 两个扩展面，但缺一个「进程内、完全信任、
   可注入 provider 协议与前端 UI」的统一载体。Revlm 提供了一个经过 grill 的成熟格式，
   迁移它的决策比重新发明便宜。

【关键洞察】
- 真正可迁移的是 6 条决策：完全信任、极简清单、目录约定即声明、文件系统为安装真相、
  冷启动生效、冲突不检测。这些与语言无关。
- 必须翻译的是 4 处机制：LD_PRELOAD 符号替换 → 注册函数；/v1 核心入口 + hook 链 →
  providerId 分发 + adapter 注册表；ChannelGroup.type → ProviderId；插件前端副作用挂载 →
  显式 register 导出。
- 必须砍掉的是 4 处语言绑定产物：原生 .so/ABI 校验、插件自管数据库 + migrate/cleanup、
  清单的 targets/requires/load_order/migrations、"插件错误阻止应用启动"。

【Linus 式方案】
1. 先定清单与目录约定（照搬 Revlm v1，删掉 targets/requires 等字段）。
2. 再定加载器：冷启动扫描 ~/.liveagent/plugins/<id>/，动态 import entry.js，调 register()。
3. 然后把现有硬编码的 provider 分发（lib/providers/llm.ts）改成注册表，让插件能注入。
4. 最后复用 Skills 已有的 stage-then-swap 安装模式做安装/更新/卸载 UX。
5. 全程零原生二进制、零沙箱、零 ABI 校验、零自管数据库——LiveAgent 不需要这些。
```

**一句话**：LiveAgent 插件 = 一个极简清单 + 一个 ESM 注册入口 + 冷启动加载，
覆盖 provider 协议适配、工具 bundle 与前端挂载三个能力面。

---

## 1. 迁移对照总表

| Revlm 概念 | LiveAgent 对应物 | 处理 |
|---|---|---|
| 完全信任的原生扩展（无沙箱/签名/白名单） | 进程内可信 ESM 扩展 | **保留** |
| 清单五字段 `id`/`type`/`name`/`description`/`version` | 相同五字段 | **保留** |
| 目录约定即声明（`backend/<arch>/`、`frontend/entry.js` 不进清单） | 目录约定即声明（`frontend/entry.js` 等不进清单） | **保留**（删掉 backend 目录） |
| 文件系统为安装真相，单目录覆盖安装，无版本并存/回滚 | `~/.liveagent/plugins/<id>/` 单目录覆盖 | **保留** |
| 冷启动生效，拒绝热加载 | 冷启动扫描 + 加载 | **保留** |
| 冲突不检测，后果由插件承担 | 复用 builtinRegistry 的"先到先得 + 告警不 throw" | **保留** |
| LD_PRELOAD 同名符号替换 + `RTLD_NEXT` 链 | 注册函数 `register(api)`，核心注册表按序收集 | **翻译** |
| `/v1` 核心唯一协议入口 → API key → ChannelGroup → hook 链 | providerId → provider adapter 注册表 → stream adapter | **翻译** |
| `revlm_register_http_routes(Server&)` 启动期一次性注册 | `api.registerProvider`（v1）；`registerToolBundle`/`registerUi`（后补） | **翻译** |
| `ChannelGroup.type` 属于组，协议分发以组为单位 | `ProviderId` 属于 provider 配置，分发以 providerId 为单位；插件 provider 的 `ProviderId = plugin:<id>` | **翻译** |
| `revlm_plugin_migrate()` / `revlm_plugin_cleanup()`（插件自管数据库） | 无插件数据库；插件经 Tauri command 用核心 store | **砍掉** |
| `core_abi` / ABI 校验 | 无 ABI 概念 | **砍掉** |
| 清单 `targets`/`requires`/`load_order`/`migrations` | 不进清单 | **砍掉** |
| 插件前端入口错误阻止控制台启动 | 加载失败仅禁用该插件，不拖垮应用 | **改写**（更宽容） |

---

## 2. 包格式

上传文件扩展名 **`.liveagent-plugin`**，内容是 ZIP。生产包至少包含清单与前端入口：

```text
plugin.json
frontend/entry.js          # 必需。ESM，导出 register()。可为空实现。
frontend/assets/...        # 可选。entry 的相对资源，走同一路径前缀。
```

- 归档路径必须是相对 POSIX 路径，不含 `..`、绝对路径、符号链接、加密项或 ZIP64；
  除必需入口外可携带任意额外文件，宿主原样保留。
- 解压校验大小、CRC 与清单后，内容先写入 staging，再**原子 rename** 到
  `~/.liveagent/plugins/<id>/`（单目录覆盖，无版本目录层级，同 Revlm v1）。
- 安装/更新只落盘，变更在**下一次冷启动**生效，运行中不改变前端与运行时。
- **与 Revlm v1 的差异**：删除 `backend/<amd|arm>/` 平台目录——LiveAgent 没有原生
  二进制插件面，前端 ESM 就是唯一执行载体，不存在双架构产物与"当前平台缺失即报错"。

安装路径复用 Skills 已成熟的 stage-then-swap 模式
（`crates/agent-gui/src-tauri/src/services/skills/install.rs`），读者永远看不到半成品。

---

## 3. 清单

`plugin.json` 只有五个字段，无 `format_version`、`core_abi`、`targets`、`requires`、
`load_order`、`migrations`（同 Revlm v1 的"清单不携带自版本字段"）：

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
  `type` 只用于分组与展示（对应 Revlm"`plugin.type` 与 `ChannelGroup.type` 是两个概念"
  的哲学，进一步简化为能力面 = 注册声明，`type` = 主类别标签）。v1 仅实现 `provider`
  能力面，`tool`/`ui` 能力面后补。

---

## 4. 前端入口与注册 API

`frontend/entry.js` 是普通 ESM，**导出 `register(api)` 函数**（不沿用 Revlm 的纯副作用
挂载——显式注册让核心能拿到能力声明，也比"靠模块副作用自挂载"更可控）：

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
- `buildStream` 在 v1 **砍掉**。仅当未来需要支持 pi-ai 不认识的私有协议时才引入。

### 加载机制（v1 定稿：候选 1）

插件 `frontend/entry.js` 经 **Tauri 资产协议动态 `import`** 加载；`register(api)` 时核心把
`api` 注入。实现细节以 Rust/WebView 实际能力为准（候选 2 仅作兜底）。

### `PluginApi` 是核心对插件暴露的**唯一表面**，等价于 Revlm 的"插件 hook 集合"：

- `registerProvider(...)`：注入 provider adapter（v1 实现）。
- `registerToolBundle(...)`：注入工具 bundle（后补，复用 `BuiltinToolBundle` 形状）。
- `registerUi(...)`：声明 UI 挂载点（后补）。
- `store`：只读写插件自身命名空间（`plugin:<id>:*`），经 Tauri command，见 §7。

---

## 5. 扩展点映射（Revlm 数据面 → LiveAgent 模型层）

Revlm 的数据面分发是：`用户 API key → ChannelGroup → ChannelGroup.type → 同名 hook 链`，
LiveAgent 的对应物是 provider 层：`providerId → adapter → stream adapter`。

| Revlm | LiveAgent 落点 |
|---|---|
| `/v1` 核心唯一协议入口 | `src/lib/providers/llm.ts`、`runtime/modelFactory.ts`、`runtime/streamByApi.ts` |
| 插件 hook（`revlm_handle_v1` 等） | 插件 provider 的 `createModel`（返回能用的 model，复用现有协议；v1 不写 `buildStream`） |
| 模型目录由插件管理 | 插件内部 `registerProvider.models`（v1 模型目录在插件内声明，不进 plugin.json） |
| `Model.id` 在 ChannelGroup.type 作用域内有效 | providerId 作用域内的模型 id 唯一；插件 provider 作用域 = `plugin:<id>` |
| 协议 usage 语义归插件 | v1 不做 usage/nativeWebSearch；插件 provider 只做纯协议流式（后补） |
| 插件可以注册普通全局端点 | v1 无 HTTP 端点面；`registerToolBundle`/`registerUi` 后补 |

**插件 provider 的协议分发**：`providerId → adapter → stream adapter` 对插件 provider 具体化为
`plugin:<id> → createModel → streamSimpleByApi`。`createModelFromConfig` 对 `plugin:<id>` 分支调用
插件注入的 `createModel`，产出 `Model` 后按 `model.api` 走现有流式管道（`streamSimpleByApi`），
核心不新增协议分支。

**与 Revlm 的核心差异**：LiveAgent 无"插件注册全局 HTTP 端点"能力（桌面应用没有
`httplib::Server` 那样的共享路由表；网关是 Go，不加载前端插件）。Revlm 的
`revlm_register_http_routes` 在 LiveAgent 里对应为 `registerToolBundle` / `registerUi`。
若未来需要插件暴露 HTTP 面，由 Rust 侧提供受控的本地 proxy 路由（`services/proxy.rs`
已有先例），不把 Go Gateway 的路由交给插件。

---

## 6. 生命周期与存储

| 操作 | 语义 |
|---|---|
| 安装 / 更新 | stage-then-swap 原子替换 `~/.liveagent/plugins/<id>/`；同 ID 覆盖，无版本并存、无手动回滚。 |
| 启用 / 禁用 | `~/.liveagent/config.sqlite` 的 `plugins` 表记录启用状态；禁用插件不参与冷启动加载。 |
| 卸载 | 删除插件目录 + 清 `plugin_data` 表中该插件的行（见下）；无 pending/cleanup 机制（无插件数据库需要清理）。 |
| 生效时机 | **下一次冷启动**。运行中不热加载、不热卸载，避免运行时状态不一致（同 Revlm ADR 0001）。 |
| 失败语义 | 单个插件加载失败（语法错误/异常/资源缺失）→ 仅禁用该插件并告警，主应用继续启动。 |

**插件管理页（v1 范围）**：本地 `.liveagent-plugin` 文件**导入**（校验 + 解压 + staging→rename）、
已装插件**列表**（含启用/禁用开关）、**卸载**。复用 `services/skills/install.rs` 的 stage-then-swap
模式（安装路径真实存在，见 `crates/agent-gui/src-tauri/src/services/skills/install.rs`）。

**插件数据（`api.store`）落点**：`config.sqlite` 新建 `plugin_data` 表，主键 `(plugin_id, key)`。
插件经 `api.store.get/set` 只读写**自身命名空间**（`plugin:<id>:*`），与其他插件/内置数据隔离。
卸载时按 `plugin_id` 删除该插件全部行。

**砍掉 migrate/cleanup 的原因**（对应 Revlm ADR 0002 的逆决策）：
Revlm 的插件各自拥有数据库表，所以需要 `revlm_plugin_migrate()`（幂等迁移、失败阻止
worker 启动）与 `revlm_plugin_cleanup()`（卸载清理、失败标记重试）。LiveAgent 的真相源
在 Rust/SQLite，插件没有独立 schema；插件数据经现有 Tauri command 走核心 store，
不存在"插件 schema 需要迁移"的问题。引入迁移框架就是为不存在的复杂性买单
——数据结构错了。

---

## 7. 安全与信任

- **完全信任，无沙箱、无签名、无能力白名单**（同 Revlm ADR 0001）。安装插件等价于
  把任意前端代码放进 WebView 进程。
- 信任半径与 Revlm 一致，但表述不同：插件能调用 `PluginApi` 表面与 Tauri command；
  **不**直接持有文件系统、Shell、SQLite 的裸权限。高权限能力仍收敛在 Rust 侧
  （沿用 LiveAgent 现有取舍：高权限能力放 Rust）。
- 工具名冲突：复用 `builtinRegistry.ts` 既有策略——第三方来源（MCP/插件）撞车时
  **先到先得、跳过后来者并告警，绝不 throw 打断整轮**；仅两侧都是可信内置组时才 throw
  （编译期开发 bug）。
- **插件协议 id 冲突**：协议 id 用 `plugin:<id>` 前缀，与内置四协议天然隔离；两个插件若
  注册相同 `plugin:<id>`，**安装时先到先得、后装者拒绝**（不静默跳过——协议 id 是安装面
  的硬冲突，区别于工具名的运行时先到先得）。
- 插件加载失败只影响该插件，不阻塞主应用（见 §6 失败语义）。

---

## 8. 与 MCP / Skills 的边界

| 扩展面 | 载体 | 信任模型 | 运行位置 | 职责 |
|---|---|---|---|---|
| **插件** | `.liveagent-plugin` | 完全信任 | 进程内（WebView） | provider 协议、工具 bundle、UI 挂载、深度定制 |
| **MCP** | `settings.mcp.servers` | 受信任配置 | 子进程/外部 server | 进程外工具协议（stdio/http/sse） |
| **Skills** | `~/.liveagent/skills` | 文档资产 | 进程内（仅提示词/文件） | 提示词资产、progressive disclosure |

边界规则：**MCP 解决"接入外部工具协议"，Skills 解决"给模型喂知识与工作流资产"，
插件解决"进程内可信代码扩展"**。三者不重叠：插件可以*提供* MCP server 配置、可以
*打包* Skills，但插件的本质能力是注册代码扩展点（provider/tool/ui）。

---

## 9. 明确砍掉的 Revlm 机制（及原因）

| 砍掉项 | 原因 |
|---|---|
| 原生 `.so` / LD_PRELOAD / `RTLD_NEXT` 链 | LiveAgent 无 C++ 内核；前端 ESM + 注册表是唯一执行模型，没有 ELF 符号替换。 |
| `core_abi` / ABI 校验 | 无 ABI 概念；插件与前端一起升级，错配由版本边界兜底。 |
| 清单 `targets` / `requires` / `load_order` | 无多架构产物、无安装/加载顺序依赖、无符号优先级冲突问题。 |
| `revlm_plugin_migrate()` / `revlm_plugin_cleanup()` | 插件不自管数据库，无 schema 迁移与卸载清理需求。 |
| 插件自管数据库 | 真相源在 Rust/SQLite；插件经 command 访问核心 store。 |
| 插件前端错误阻止应用启动 | 降级为"仅禁用该插件"，不拖垮主应用（比 Revlm 更宽容）。 |
| 多版本并存 / 回滚 / 沙箱 / 签名 | 产品不需要（同 Revlm ADR 0001 已否决）。 |
| 热加载 | 冷启动生效，避免运行时状态不一致（同 Revlm ADR 0001）。 |
| 插件写 `buildStream`（私有协议适配） | v1 复用现有 `model.api` 协议；`buildStream` 仅当未来需私有协议时引入。 |
| 插件 provider 的 usage 查询 / 原生搜索 | v1 只做纯协议流式；usage/nativeWebSearch 是内置 provider 增值能力，后补。 |

---

## 10. 实施路径（后续里程碑）

| 阶段 | 内容 | 关键文件 | v1 |
|---|---|---|---|
| 1 | 定清单 + 目录约定 + `PluginApi` 类型 | 新建 `src/lib/plugins/*` | ✅ |
| 2 | 冷启动扫描加载器（扫描目录 → import entry → register） | Rust `services/plugins.rs`、前端 `lib/plugins/*` | ✅ |
| 3 | provider 分发改造（`createModelFromConfig` 对 `plugin:<id>` 分支调插件 `createModel`） | `lib/providers/llm.ts`、`runtime/modelFactory.ts` | ✅ |
| 5 | 插件管理页（导入 + 列表 + 启停 + 卸载，stage-then-swap） | `services/skills/install.rs` 模式复用 | ✅ |
| 4 | 工具 bundle 注册（复用 `BuiltinToolBundle`） | `lib/tools/builtinRegistry.ts` | 后补 |
| 6 | UI 挂载点 | Settings/Plugins 页、Hub | 后补 |

---

## 11. v1 决策账本（2026-08-03 grill 已定）

| 决策点 | v1 定论 |
|---|---|
| 落地范围 | provider 注入闭环 + 管理页 CRUD（阶段 1-3 + 5 UX）。tool/ui、proxy、usage 后补。 |
| 安装路径 | 管理页本地导入 `.liveagent-plugin`（校验 + 解压 + staging→rename）。 |
| 管理页功能 | 导入 + 列表 + 启停 + 卸载。 |
| 插件运行时面 | 纯前端 ESM + 核心内建库（Tauri 无运行期动态加载编译插件，无稳定 ABI）。 |
| `api` 能力 | `registerProvider` + `store`（只读写自身命名空间）。`registerToolBundle`/`registerUi` 占位后补。 |
| `type` 枚举 | v1 只认 `provider`；`tool`/`ui` 安装时拒绝。 |
| 插件 = 协议 | 插件 provider 是 `plugin:<id>` 协议，供应商配置里与内置四协议并列；一协议可被多 provider 实例复用。 |
| 模型目录来源 | 插件内部 `registerProvider.models`，不进 plugin.json；协议 tab 预填，实例共享初始列表。 |
| 模型进选择器 | 协议 tab 预填（不合并进全局目录，不合成 customProviders 条目）。 |
| `buildStream`/`createModel` 接缝 | 插件自管 Model 构造；`createModel` 返回可用 model（`api ∈` 现有四协议），核心走 `streamSimpleByApi`；**不写 `buildStream`**。 |
| 协议 id | `plugin:<plugin.json.id>`；一个插件包绑定一个协议。 |
| 数据持久化 | `config.sqlite` `plugin_data` 表，主键 `(plugin_id, key)`，只读写自身命名空间。 |
| HTTP 端点面 | v1 不做（无 `api.proxy`）。 |
| 签名/来源校验 | 不校验（完全信任）。 |
| 协议 id 冲突 | `plugin:<id>` 前缀与内置隔离；两插件撞 `plugin:<id>` → 安装先到先得、后装拒绝。 |

---

## 参考

- Revlm 包格式 v1：`/Users/realm/revlm/docs/plugin-package-format.md`
- Revlm 插件上下文（术语）：`/Users/realm/revlm/CONTEXT.md`
- Revlm ADR：`/Users/realm/revlm/docs/adr/0001-0005`
- Revlm 决策账本：`/Users/realm/revlm/docs/plugin-design-decision-ledger.md`
- LiveAgent 工具注册：`crates/agent-gui/src/lib/tools/builtinRegistry.ts`、`docs/features/tools.md`
- LiveAgent Skills 安装（stage-then-swap）：`crates/agent-gui/src-tauri/src/services/skills/install.rs`、`docs/features/skills-and-mcp.md`
- LiveAgent provider 层：`crates/agent-gui/src/lib/providers/llm.ts`、`runtime/*`
- LiveAgent 供应商配置 UI：`crates/agent-gui/src/pages/settings/ProvidersSection.tsx`
