# Mocker 桌面端设计

日期：2026-09-03  
状态：待实现  
方案：C — 项目台。Rust + GPUI Kit（`gpui-component`）桌面端，进程内 mock HTTP 服务。视觉跟 Monet Paper。

## 1. 目标

给前端本机联调用。后端还没好时，把公司风格的接口说明粘贴进桌面端，生成可调用的 mock，前端把 baseURL 指到 `http://127.0.0.1:<端口>` 即可把页面跑通。

第一版必须能：导入并改接口、按项目分类、多项目多端口同时跑、点选场景、改 JSON 和字段、看请求日志、自定义成功码/失败码。

非目标（第一版明确不做）：PDF/Word 导入、按入参自动分支、有状态 CRUD、真鉴权、延迟/断网模拟、OpenAPI、团队共享服务、模块树、客户端内「试发请求」、Cursor / Gemini 登录态适配。

## 2. 用户与成功标准

主用户：前端。日常路径是「项目首页 → 新建项目 → 工作台导入或手工编辑 → 启动 → 前端改 baseURL → 点场景 / 改 JSON → 看日志」。模型来源在**设置页**，导入前配置一次即可。

用「智能话机」设计文档里 2～3 个真实接口走通下列验收（含 header 里的 `sn`、出参嵌套 `3.1`、带 `list/total` 的列表）：

1. 新建项目，自定义成功码/失败码、端口、默认头 `sn`。
2. 粘贴导入 → 预览勾选 → 写入。
3. 启动后，curl 或前端打 `http://127.0.0.1:<端口><完整路径>`，成功场景结构正确，且 mock 数据按字段语义可看（不是全 `0` / `"示例"`）。
4. 客户端点切空数据 / 参数错误 / 业务失败，下一发请求立刻变，不用重启。
5. 改成功 JSON 或响应字段树，热更新。
6. 手工新建一个接口也能调通。
7. 日志能看到命中、未命中、是否缺默认头。
8. 第二个项目另一个端口同时可调。
9. 关掉应用再开：项目、接口、场景、当前场景、成功码还在；服务为停止状态，需手动再启动。

## 3. 架构

一个 Rust 进程。GPUI 画界面；tokio 跑 HTTP。每个「正在运行的项目」在进程内绑自己的端口，不另起子进程。

| 单元 | 做什么 | 不做什么 | 依赖 |
|---|---|---|---|
| UI | 项目、导入预览、接口编辑、场景点选、字段/JSON、日志 | 不 listen、不拼 SQL、不直接读模型 Key 文件 | 应用服务 |
| 应用服务 | 创建项目、导入确认、启停、切场景、改字段/JSON、选模型来源 | 不解析提示词细节、不画控件 | 存储、解析器、运行时、来源扫描 |
| 来源扫描 | 读本机 Claude Code / Pi / Codex / Grok 配置，列出可用来源 | 不把密钥写入 SQLite | 本机文件 |
| 解析器 | 把粘贴文本 + 模型输出变成接口草稿；程序生成空/错误场景 | 不写库、不启动服务 | 当前模型来源 |
| 运行时 | 按路由快照响应；写请求日志 | 不渲染、不改场景定义（只读快照） | 存储快照 |
| 存储 | SQLite 持久化项目/接口/场景/日志/全局设置 | 不发起 HTTP | 本地文件 |

数据流：

```
粘贴文本 → 当前模型来源 → 结构化接口 + 成功 JSON
        → 程序生成空/错误场景 → 预览（可改）→ 确认写入 SQLite
        → 启动项目 → 运行时加载快照 → 127.0.0.1:端口
改场景/JSON/路径 → 替换快照 → 下一请求生效
请求进入 → 匹配 → 返回当前场景 → 追加日志
```

技术选型：

- UI：[GPUI Kit](https://gpui-kit.com/zh-CN/) 的 `gpui-component` 0.5（crates.io）。`gpui_component::init` + 每窗 `Root::new`。组件用 Button / Input / Textarea / Switch / Select / List，不用 Dock。
- 视觉：Monet Paper token 映射到 `cx.theme()`（卡纸底 `#F2EBDC`、墨绿主色 `#3A5A40`、砖红 `#A4453B`、纸片阴影）。设置里可切 Ink。
- HTTP：axum + tokio，只绑 `127.0.0.1`
- 存储：SQLite（macOS：`~/Library/Application Support/Mocker/mocker.db`）
- 快照：可原子替换的路由表（`ArcSwap` 或等价），避免改配置时打断进行中的请求

## 4. 领域模型

### 4.1 全局设置（一行）

- `selected_source_id`：当前模型来源，例如 `claude-code`、`pi:xsy-llm`、`codex:cc-switch-official`、`grok:grok-4.5`、`manual`
- `selected_model`：该来源下的模型名
- 手动来源（仅 `manual` 使用）：`base_url`、`api_key`、`protocol`（`openai-chat` 或 `anthropic-messages`）、`model`
- 密钥：手动来源的 key 可存在 SQLite；扫描来源的 key **永不写入**，每次请求现场读原配置文件

所有项目共用这一套来源，项目级不能覆盖。

### 4.2 项目

- `id`、`name`
- `port`：正整数，用户指定
- `success_code`、`fail_code`：以字符串存储，默认 `"0000"` / `"9999"`
- `default_headers`：`[{ key, value }]`，例如 `sn`。`value` 只给界面对照，运行时不注入、不校验
- 运行状态不持久化：进程退出即视为停止。只有 `enabled=true` 的接口进入该项目的路由快照

成功码/失败码写入 JSON 的规则：

- 匹配 `^-?(0|[1-9][0-9]*)$`（无前导零）→ JSON number，如 `200`
- 其他（含 `0000`）→ JSON string

改项目成功码/失败码 **不改已经保存的场景 JSON**。要同步：对该接口点「按字段重新生成该场景」，或「用 AI 重新生成成功数据」。运行时现拼的包（404/405/500）用**当前**失败码。

### 4.3 接口

- `id`、`project_id`
- `method`：缺省 `POST`，大小写在匹配时统一成大写
- `path`：完整 path，精确匹配，不含 host、不含 query。必须以 `/` 开头
- `name`、`notes`（可存接口逻辑原文）
- `source_text`：导入本批时的完整粘贴原文（同一批多接口共享同一份），供以后再用 AI 生成
- `deprecated`：标题或描述含「废弃」时导入为停用
- `enabled`：停用视作未注册
- `current_scene`：`success` | `empty` | `param_error` | `business_error`

同一项目内 `(method, path)` 唯一。

### 4.4 字段

每条接口两套字段：

- 入参：`location` = `header` 或 `body`
- 出参：`location` = `response`，用 `children` 表示 `3.1` / `3.2.1` 嵌套

字段属性：`name`、`name_zh`、`type`（`String` / `Integer` / `Number` / `Boolean` / `Object` / `Array` / `Null`）、`required`、`comment`、`enum_values`。

入参表只用于展示、编辑、给模型当上下文。第一版运行时 **不按入参做校验分支**。参数错误必须靠点选「参数错误」场景。

### 4.5 场景

每个接口固定 4 个场景，缺一不可：

| 场景 id | 界面文案 | HTTP | body |
|---|---|---|---|
| `success` | 成功 | 200 | 模型生成的语义化 JSON；导入后可手改 |
| `empty` | 空数据 | 200 | 完整信封。`code` 用项目成功码，`msg` 为 `"成功"`。`data`：若成功 JSON 的 data 含 `list`/`total`，则保留其它键的类型默认值并把 `list` 置 `[]`、`total` 置 `0`；若 data 是数组则 `[]`；否则 `{}` |
| `param_error` | 参数错误 | 400 | `{ code: 失败码, msg: "参数错误", data: null }` |
| `business_error` | 业务失败 | 200 | `{ code: 失败码, msg: "失败", data: null }` |

每个场景存：`http_status`、`body_json`（文本，允许暂时不合法的 JSON）。

**响应 JSON 是前端真正拿到的东西。** 响应字段树是同一份 JSON 的视图：改树写回 JSON；改 JSON 能解析则刷新树；不合法时文本仍保存，树只读并提示。该场景仍按原文返回。

界面上的「当前场景」选项既是编辑对象，也是运行时生效对象。点一下就切，不另做预览场景。

### 4.6 请求日志

字段：时间、项目 id、方法、完整 URL、请求头、请求体原文、是否命中、接口 id、场景 id、响应码、响应体、耗时毫秒、`missing_default_headers`。

每项目最多 500 条，超出删除最旧。未命中也记。

## 5. 模型来源扫描

设置里提供「模型来源」：扫描、刷新、选择一个、测试连通。

### 5.1 可用来源（第一版）

来源必须已经具备 **Base URL + 凭证**，或 **本机 OpenAI 兼容反代正在监听**。

| 来源 | 读哪里 | 怎么判定可用 |
|---|---|---|
| Claude Code | `~/.claude/settings.json` 的 `env.ANTHROPIC_BASE_URL`、`env.ANTHROPIC_AUTH_TOKEN`、`env.ANTHROPIC_MODEL`（或 `model` 别名） | Base URL 与 Token 都非空。协议：`anthropic-messages` |
| Pi | `~/.pi/agent/models.json` 的 `providers.*` | 有 `baseUrl` 且有 `apiKey`。`api` 为 `anthropic-messages` 或 OpenAI 兼容（`openai-completions` / `openai-chat`） |
| Codex | `~/.codex/config.toml` 的 `model`、`model_provider`、`[model_providers.<id>]` | `base_url` 存在。协议：`openai-chat`。若 host 是 `127.0.0.1`/`localhost`，扫描时 TCP 探活，不通则标不可用 |
| Grok CLI | `~/.grok/config.toml` 里带 `base_url` 与 `api_key` 的 `model` 段 | 两者都非空。协议：`openai-chat` |

扫描列表同时给出可用与不可用项：可用可选；不可用灰显并写原因（缺 Token、本地端口未开）。

### 5.2 明确不可用（第一版只展示原因，不发起请求）

- Cursor 登录态
- Gemini CLI 仅 OAuth、无独立可复用 HTTP 时
- Pi 的 `openai-codex` 等纯 refresh token 登录
- Claude 仅 `~/.claude.json` oauth、且 settings 里没有 Token

### 5.3 凭证与请求

- 扫描来源：请求时打开原文件读凭证，不缓存到磁盘，内存只保留到这次导入结束。
- 不把粘贴的接口说明写到除本机 SQLite 以外的地方；模型请求发往用户选中的来源（可能是公司网关或公网）。
- 手动来源：设置里填 Base URL、API Key、协议、模型，作为扫描为空时的逃生口。
- 「测试连通」发一条最小请求，成功则提示模型名，失败展示状态码/错误文本。
- 发请求路径：`openai-chat` / `openai-completions` 一律 POST 到 `{base}/chat/completions`；`anthropic-messages` 一律 POST 到 `{base}/v1/messages`。`base` 取配置里的 Base URL，去掉末尾 `/`；若已以 `/v1` 或 `/v1/messages` 结尾则不再重复拼接。

## 6. 粘贴导入

第一版只支持粘贴文本（公司设计文档骨架：接口名称、路径、POST、入参表、出参表）。不解析 PDF/Word。

### 6.1 流水线

1. 用户在当前项目打开导入弹层，贴文本。
2. 若未选模型来源或来源不可用：不能点解析，提示去设置。
3. 把原文交给模型，要求 **只返回一个 JSON 对象**（不要 markdown 围栏）。JSON 形状：

```json
{
  "endpoints": [
    {
      "name": "首页数据概览",
      "path": "/hl/pub/phone/v1/queryPhoneHomeData",
      "method": "POST",
      "deprecated": false,
      "notes": "接口逻辑原文，可空",
      "request_headers": [
        {
          "name": "sn",
          "name_zh": "设备sn号",
          "type": "String",
          "required": true,
          "comment": "header",
          "enum_values": []
        }
      ],
      "request_body_fields": [],
      "response_fields": [],
      "envelope": true,
      "success_body": {}
    }
  ]
}
```

`response_fields` 与 body 字段使用同一字段对象，嵌套放在 `children`。若文档出参是 `code` / `msg` / `data` + `3.1` 挂在 data 下：`envelope` 为 true，`success_body` 必须是完整信封，且 `code` 使用**当前项目成功码**（提示词里传入项目的成功码/失败码）。

4. 程序校验模型 JSON：能解析、`endpoints` 为非空数组、每条有非空 `path` 且以 `/` 开头、`method` 为 GET/POST/PUT/PATCH/DELETE。`success_body` 必须是 JSON 对象。失败则停在预览错误区，原文保留，不写入项目。导入后成功场景 JSON 以 `success_body` 为准；`response_fields` 填字段树，两者不一致时成功场景仍以 JSON 为准。
5. 程序补 3 个非成功场景（见 4.5），不二次调用模型。
6. 预览列表：勾选、改路径/方法/名称、看字段和成功 JSON。路径为空、与项目内或本批内 `(method, path)` 重复的标红，不能确认。
7. 确认后写入当前项目。标题含「废弃」的默认 `enabled=false`，`current_scene=success`。

### 6.2 提示词约束（实现时按此写死）

- 按语义填成功数据：手机号像手机号、枚举用文档里的合法值、列表 2～3 条、时间用 ISO 或文档格式。
- 不编文档里没有的字段。
- 多接口一次切分。
- `sn` 等写在描述里的 `header` 必须进 `request_headers`。
- 不要返回解释性文字。

### 6.3 导入后再生

- 「按字段重新生成该场景」：程序按字段树 + 项目成功码/失败码覆盖**当前正在编辑的那一个场景**。成功场景会变成类型默认值（失去语义化），因此按钮需二次确认。
- 「用 AI 重新生成成功数据」：把该接口 `source_text`（没有则用字段树 JSON）再送模型，只替换成功场景 body，其它场景不动。

手工新建接口：用户填方法、路径、名称 → 四个场景用程序默认信封（成功 `data: {}`）→ 再自己改 JSON/字段或点 AI 生成。

## 7. Mock 运行时

- 每个运行中的项目：`127.0.0.1:<port>`。未运行不占端口。
- 端口冲突：启动失败，提示端口和 `Address already in use`，**不改成别的端口**。
- 匹配：`METHOD + path` 精确匹配。第一版无 path 参数、无前缀、query 不参与路由。
- 停用接口 = 未注册。
- 未命中：HTTP 404，`{ "code": <失败码>, "msg": "未找到 mock 接口", "data": null }`。
- path 在、方法不对：HTTP 405，同一信封，`msg` 为 `"方法不允许"`。
- 运行时 500：同一信封，`msg` 为 `"mock 内部错误"`。
- CORS：`Access-Control-Allow-Origin: *`；`Access-Control-Allow-Methods: GET, POST, PUT, PATCH, DELETE, OPTIONS`；`Access-Control-Allow-Headers: *`。`OPTIONS` 返回 204，不进业务匹配、不写日志。
- 项目默认头不校验、不拦截。请求缺了声明的头仍返回当前场景；日志标记缺失键名。
- Content-Type 响应为 `application/json; charset=utf-8`。请求体不是 JSON 也返回当前场景，体以原文记日志。
- 不读 cookie，不做登录态，不模拟延迟/限流。
- 热更新：改场景、JSON、路径、方法、启停接口后替换快照；进行中的请求用进入时的快照。
- 单请求处理异常：该请求 500（见上），进程和其它项目继续。

## 8. 界面（方案 C · 项目台）

一个主窗口，用页面切换而不是 Dock。HTML 对照：`docs/superpowers/ui/index.html`。

全局顶栏：窗口控件 + `Mocker` + **设置**。模型来源不出现在工作台。

**无项目**：居中文案 + 主按钮「新建项目」。

**项目首页**：左标题「项目」，右上主按钮「新建项目」（不能只靠虚线卡片）。先「运行中」卡片，再「全部项目」卡片。卡片展示名称、运行状态、端口、接口数；点击进入工作台。

**新建项目页**：← 返回项目。表单：名称、端口、成功码（默认 `0000`）、失败码（默认 `9999`）、默认请求头。创建后打开该项目工作台。

**设置页**：← 返回上一页。只放低频项：扫描到的模型来源（单选）、刷新扫描、测试连通、手动填写兼容接口、外观 Paper / Ink。所有项目共用这一套来源。

**工作台**：← 项目。当前项目名、运行状态、端口、启停、导入、项目设置。右侧常驻「已写入 · 下一请求生效」。

- 左：搜索、接口列表（方法徽章 + 名称 + path 末段）、底部「手工新建接口」。
- 右：全部可编辑并立刻写库 + 热更新快照——名称、启用、方法、完整路径、当前场景、Header 表、Body 表、响应字段表、场景 JSON。表可增删行。按钮：「按字段重新生成该场景」（二次确认）、「用 AI 重新生成成功数据」。
- 底：请求日志，最新在上，可清空。

**导入页**：← 工作台。大文本框、解析（可取消）、预览勾选、写入当前项目。使用设置里的当前模型来源。

**项目设置**：名称、端口、成功码、失败码、默认请求头表。改码不自动改已存 JSON。

接口字段、路径、JSON 的每次有效编辑：写入 SQLite，若项目在跑则替换路由快照，**不重启端口**。JSON 不合法仍保存原文，字段树只读。

第一版不做：多窗口、主题市场、客户端内试发请求、模块树。

## 9. 出错处理

- 解析、保存、启停失败用界面提示，不崩进程。
- 模型超时、非 JSON、缺 path：停在导入预览错误区，不写库。
- 来源文件读失败或密钥为空：该来源标不可用。
- SQLite 打不开：提示数据目录异常；已在跑的 mock 尽量继续，避免正在联调被带停。
- JSON 不合法：允许保存原文，树只读。
- 导入路径空或重复：预览标红，不能确认，不静默加后缀。
- 只绑环回地址；不上传日志到远程；日志有 500 条上限。

## 10. 测试

不强制 GPUI 点选自动化。必须有：

1. **来源扫描**：用临时目录里的伪造 `settings.json` / `models.json` / `config.toml` 测可用/不可用判定；断言密钥不会出现在序列化后的全局设置（`manual` 除外）。
2. **模型 JSON 校验**：合法草稿通过；缺 path、非 JSON、空数组有明确错误。
3. **场景生成**：信封、`0000` 为字符串、`200` 为数字、空列表 `list/total`、参数错误 400。
4. **运行时**：绑临时端口；命中成功/切场景；404/405；OPTIONS 204；热更新后下一请求变；默认头缺失只记日志不拦截。

手工验收用仓库内 `设计文档v1.0.0.0.pdf` 转出的接口文本（至少首页概览、带 list 的通话记录列表、带 header `sn` 的修改类接口）。

## 11. 实现边界（给后续计划用）

建议实现顺序，每一段都应可单独跑测试：

1. SQLite 与领域读写（项目/接口/场景/日志/全局设置）
2. 运行时 + 快照热更新 + HTTP 测试
3. 来源扫描 + 模型调用适配（Anthropic / OpenAI 兼容）
4. 导入校验与场景补全（可先用固定模型假响应测）
5. GPUI Kit 壳：Paper 主题、无项目、项目首页、新建项目、设置
6. 工作台：列表、可编辑字段/JSON、启停、热更新、日志、导入页

单元之间只通过明确结构体通信：`Project`、`Endpoint`、`Scene`、`Field`、`ModelSource`、`ImportDraft`、`RouteSnapshot`。UI 不得直接打开 `~/.claude/settings.json`。
