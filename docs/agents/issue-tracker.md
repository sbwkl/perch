# Issue tracker: GitHub

本仓库的 issue 与规格存放在 GitHub Issues（`sbwkl/perch`）。所有操作使用 `gh` CLI。

## 约定

- **创建 issue**：`gh issue create --title "..." --body "..."`；多行正文用 heredoc。
- **读取 issue**：`gh issue view <number> --json number,title,body,labels,comments`。
- **列出 issue**：`gh issue list --state open --json number,title,body,labels,comments --jq '[.[] | {number, title, body, labels: [.labels[].name], comments: [.comments[].body]}]'`，配合 `--label` / `--state` 过滤。
- **子 issue**：`gh issue create --parent <parent> ...`，或 `gh issue edit <parent> --add-sub-issue <child>`（`gh` 2.94+）。旧版：`gh api --method POST repos/<owner>/<repo>/issues/<parent>/sub_issues -F sub_issue_id=<child-db-id>`。不支持子 issue 时，在子 issue 正文顶部写 `Part of #<parent>`。
- **阻塞关系**：用 GitHub 原生 issue dependencies（UI 可见）：`gh issue create --blocked-by <n>,<n> ...` / `gh issue edit <child> --add-blocked-by <blocker>`（`gh` 2.94+）。旧版：`gh api --method POST repos/<owner>/<repo>/issues/<child>/dependencies/blocked_by -F issue_id=<blocker-db-id>`，其中 db-id 取自 `gh api repos/<owner>/<repo>/issues/<n> --jq .id`。不可用时退化为正文顶部 `Blocked by: #<n>`。所有 blocker 关闭即解除阻塞。
- **统计未关闭 blocker**：`gh api repos/<owner>/<repo>/issues/<n> --jq .issue_dependencies_summary.blocked_by`（仅未关闭；0 表示可开工）。
- **评论**：`gh issue comment <number> --body "..."`。
- **打 / 去标签**：`gh issue edit <number> --add-label "..."` / `--remove-label "..."`。
- **关闭**：先 `gh issue comment` 说明，再 `gh issue close <number>`。对已关闭 issue 用 `gh issue close --comment` 会丢评论但仍返回 0。

仓库由 `git remote -v` 推断；在克隆目录内 `gh` 自动识别。只往用户自己的仓库提 issue；fork 中 `gh` 可能默认指向上游，首次 `gh issue create` 前用 `gh repo view --json nameWithOwner` 确认。

## Pull request 作为 triage 入口

**PR 作为请求入口：no。** _(若本仓库把外部 PR 当特性请求，改为 `yes`；`/triage` 读取此标记。)_

设为 `yes` 时，PR 走与 issue 相同的标签与状态，用 `gh pr` 等价命令：

- **读 PR**：`gh pr view <number>`，再 `gh pr view <number> --comments`，以及 `gh pr diff <number>`。
- **列出待 triage 的外部 PR**：`gh api --paginate 'repos/{owner}/{repo}/pulls?state=open' --jq '.[] | select(.author_association | IN("OWNER","MEMBER","COLLABORATOR") | not) | {number, title, author: .user.login, author_association, labels: [.labels[].name]}'`。
- **评论 / 打标签 / 关闭**：`gh pr comment`、`gh pr edit --add-label`/`--remove-label`、`gh pr close`。

GitHub 的 issue 与 PR 共用编号，裸 `#42` 可能是任一者：先 `gh pr view 42`，失败再 `gh issue view 42`。

## 当技能要求“发布到 issue tracker”

创建 GitHub issue。

## 当技能要求“获取相关 ticket”

按上文 **读取 issue**。

## 寻路（wayfinder）操作

供 `/wayfinder` 使用。**地图**是一个 issue，**子 ticket** 为其子 issue。

- **地图**：带 `wayfinder:map` 标签的 issue，正文承载 Notes / Decisions-so-far / Fog。`gh issue create --label wayfinder:map`。
- **子 ticket**：以 GitHub 子 issue 挂到地图（见上文 **子 issue**）。不支持子 issue 时，在地图正文的任务列表里登记，并在子 issue 正文顶部写 `Part of #<map>`。标签 `wayfinder:<type>`（`research`/`prototype`/`grilling`/`task`）。认领后 assign 给执行者。
- **阻塞**：见上文 **阻塞关系**。
- **前沿查询**：列出地图的未关闭子 issue，去掉有未关闭 blocker（见 **统计未关闭 blocker**，或 `Blocked by` 行中的未关闭 issue）或有 assignee 的；地图顺序最前者胜出。
- **认领**：`gh issue edit <n> --add-assignee @me`，本次会话的首次写操作。
- **解决**：`gh issue comment <n> --body "<答案>"`，再 `gh issue close <n>`，然后把上下文指针（要点 + 链接）追加到地图的 Decisions-so-far。