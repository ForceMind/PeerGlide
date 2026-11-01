# MouseInc / Aitiy 研究与屏跃取舍

研究日期：2026-11-01。用途：为 PeerGlide 的增量路线提供证据，不是移植承诺。

## 1. 方法与可信度

本轮阅读原作者设置仓库、随仓库发布的配置/动作文档、Aitiy 官网/用户手册/API，以及屏跃当前源码、开发文档与 CI。未安装或实测两个竞品，未取得其完整原生核心源码；不提供虚构的性能、识别率或安全审计结论。官方功能描述代表厂商声明，不等于本项目已经验证。

证据分级：A = 一手源码/官方文档明确说明；B = 有限材料支持但平台、版本或实现范围不完整；U = 未确认。未发现不等于不存在。

## 2. 技术与产品边界

| 事项 | MouseInc | Aitiy | 对屏跃的决定 |
|---|---|---|---|
| 可检查源码 | MouseInc.Settings 是设置界面，不是输入核心（A） | 本轮未取得完整核心源码（U） | 不以两者为代码依赖 |
| 已确认技术 | 设置仓库 README 为 Vue 3/Vite/Element Plus；不能外推核心语言（A） | 官方正式版记录为自研原生 UI、Lua 动作；最新核心完整语言组成未确认（A/U） | 保留 Rust + eframe，不为竞品重写 UI 栈 |
| 系统 | 已查资料面向 Windows；未确认官方 Mac 版本（B） | 官网提供 Windows/macOS；支持客户端不代表所有功能同等支持（A） | Windows/macOS 双向协同必须自己验收 |
| 规则模型 | 手势数据、全局/应用规则、动作序列分离（A） | 多类触发器共享动作库（A） | 共享 Trigger/Rule/Action，不为每个入口复制逻辑 |
| 多机能力 | 已查配置文档未描述内置协同（B） | Infinite Mouse 手册明确讲 Windows 局域网；官网日志提到文件拖拽（A） | 参考配对与布局交互，不宣称掌握其传输/安全实现 |
| 扩展与迁移 | JSON 配置与动作列表可用于理解数据模型（A） | 正式版迁移要求重建旧 Alpha 动作/规则（A） | 配置版本、迁移预览、备份和回滚必须先设计 |

纠正此前讨论口径：早期二手介绍的“C++ + WebView2”不能充当 Aitiy 当前完整技术栈证据；MouseInc 的设置源码公开不等于整个产品开放源码。两个产品都不能在未核实授权时直接复制核心、界面或预设资产。

## 3. MouseInc 的可复用产品思想

原作者配置文档分别定义 `Gestures`、`MatchGlobal`、`MatchCustom`、`Actions`，并将全局热键、边缘滚轮与热角绑定到动作。它还暴露触发距离、超时、识别失败恢复策略、排除程序和忽略全屏选项。[S2]

对屏跃的启发不是增加很多按钮，而是三个层次：输入触发方式可替换；应用条件决定规则；动作描述用户意图。应用规则可以复用一组行为，不必每个应用从零配置。动作文档明确以手势起点窗口为目标，这提醒我们保存触发上下文，而不是结束后随意选择前台窗口。[S3]

取舍：G1 优先少量高频手势、应用排除、恢复普通右键、暂停和可解释的规则匹配。边缘滚轮、热角在跨屏仲裁成熟后加入；输入统计、按键音效等不占用核心协同开发路径。不得照搬旧文档中依赖提高权限或弱化系统防护的解决方式。

## 4. Aitiy 的启发与不能下的结论

官网可检索日志记录：2026-09-15 正式版切换原生 UI/Lua；09-21 增加 Infinite Mouse 文件拖拽；09-24 首个 Mac 版本；可检索条目含 10-13 的 v1.0.8。官网动态页面可能晚于搜索索引更新，本记录不把索引版本号当成长期“最新版本”。[S4]

官方手册把触发器与动作库连接起来，并说明脚本资源限制、触发上下文和迁移边界。屏跃借鉴其统一动作入口和受限执行思路；具体配额、API 和实现独立设计。Windows Infinite Mouse 的描述不能证明 Win↔Mac 功能对等，也不能证明其加密、认证或重放防护实现。[S5]

结论：Aitiy 已覆盖“鼠标增强 + 部分多机协同”的产品组合。屏跃的定位应是经验证的跨系统协同、可审计开源实现、可独立使用的本机增强，而不是声称行业首次。

## 5. 研究结果到研发条目的映射

| 观察 | 屏跃需求 | 排期/跟踪 | 本轮状态 |
|---|---|---|---|
| 同一动作接多个触发器 | 类型化动作目录、能力查询、规则优先级 | G1 / #11 | 仅规划 |
| 普通右键与手势竞争 | Pending/Candidate 状态、平衡按键恢复、失败不乱回放 | G1 / #11 | 仅规划 |
| 起点窗口语义 | 本地目标快照、远端不透明上下文令牌 | G1/G2 / #11 #12 | 仅规划 |
| 配对与屏幕布局 | 复用现有 M2/M3，不另建连接系统 | #4 #5 | 现有任务延续 |
| 多机数据协同 | 先剪贴板/显式发送，原生拖拽另验 | M4 / #6 | 尚未实现 |
| 原生 UI 与轻量体验 | 保留 eframe；覆盖层走独立适配接口；测量资源开销 | G1/M5 | 不换框架 |
| 动作脚本 | 白名单宿主接口、资源限制、隔离进程研究 | 后续扩展 | 不进首版关键路径 |
| 配置升级破坏兼容 | schema_version、预览、备份、回滚、不导入秘密 | G1/M3 | 尚未实现 |

## 6. 许可与安全复用规则

仅参考公开行为和数据结构思想，自主实现。MouseInc.Settings README 的笼统许可描述不能替代逐文件许可证核验；本轮未复制其源码、文案、图片、动作预设。Aitiy 未提供本轮可审计的完整核心，不反向假设可复用。已有 MIT 与 GPL 参考隔离策略继续有效，见 `REFERENCE_RESEARCH.md`、`CONTRIBUTING.md`。

可研究只读导入用户主动选择的 MouseInc 配置：先验证格式/大小，展示映射报告，仅白名单转换手势、应用规则和基础语义动作；未知命令保留为禁用条目。导入不执行程序、脚本、网址，也不修改原文件。Aitiy 未确认稳定导出格式前，不承诺一键完整迁移。

## 7. 后续竞品验证脚本（尚未执行）

统一设备和任务：普通右键、右键拖动、8 种常用手势、快速滚轮、应用切换、DPI变化、全屏退出、权限拒绝、睡眠恢复。多机项目仅测其官方支持组合，记录每端版本、OS、输入设备、网络、步骤、结果与失败录像；不把 Windows 结果外推 Mac。

比较维度是误触、取消恢复、目标窗口正确性、跨屏冲突、配置迁移和可理解性。资源占用需相同机器、采样窗口与后台条件，先记录绝对值再比较；不凭语言或安装包大小断言性能。

## 8. 一手来源与本轮证据

- [S1] 原作者设置仓库与 README：https://github.com/shuax/MouseInc.Settings 。仓库页面显示 2026-09-18 归档；归档仅证明该仓库状态。
- [S2] 配置文档：https://github.com/shuax/MouseInc.Settings/blob/main/docs/docs/config.md 。本轮 blob `15fd8e1128ea380ec1cadc683680c97bca872a48`。
- [S3] 动作文档：https://github.com/shuax/MouseInc.Settings/blob/main/docs/docs/action.md 。本轮 blob `8a5e84dae009efa7d89f224fc6eb6432cbf619ac`。
- [S4] Aitiy 官方网站与更新记录：https://aitiy.com/ 。访问日期 2026-11-01，动态页与搜索索引存在展示差异。
- [S5] Aitiy 官方手册/Lua API：https://aitiy.com/docs.html 。明确限定本手册为 Windows。
- [S6] MouseInc 官方手册：https://docs.shuax.com/MouseInc/ 。直接解析为空，本轮以原作者仓库文档补足，不以转载替代。
- [S7] Windows hook 回调约束：https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelmouseproc 。回调不得等待网络、脚本、OCR或UI。
- [S8] SendInput 权限边界：https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput 。不绕过 UIPI。
- [S9] Apple Quartz Event Services：https://developer.apple.com/documentation/coregraphics/quartz-event-services 。具体权限按使用的 API 与实机结果建立矩阵。
- [S10] 手势算法研究入口：https://depts.washington.edu/acelab/proj/dollar/ 。仅用于方法比较；方向型手势不得直接使用旋转不变归一化，任何代码引入另审许可。

后续产品路线及任务见 [PRODUCT_EVOLUTION.zh-CN.md](PRODUCT_EVOLUTION.zh-CN.md)，输入架构决定见 [ADR-0002](ADR/0002-unified-input-actions.md)。
