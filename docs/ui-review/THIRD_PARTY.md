# 客户样板的开源组件来源

| 来源 | 版本与许可 | 实际使用 |
| --- | --- | --- |
| [Shadcn Admin](https://github.com/satnaing/shadcn-admin/tree/e16c87f213a5ba5e45964e9b67c792105ec74d26) | `e16c87f213a5ba5e45964e9b67c792105ec74d26`，MIT | 适配 `AppSidebar` 的品牌区、导航、页脚组合和数据表格行菜单。参考其分页及列可见性实现，将 FTW 的分页和列配置接回现有 Core 查询。 |
| [shadcn/ui Radix 组件源码](https://github.com/shadcn-ui/ui/tree/055aa6b7837b1808456a24beb4c97a677864aa82/apps/v4/registry/new-york-v4/ui) | `055aa6b7837b1808456a24beb4c97a677864aa82`，MIT | 将 Dropdown Menu、Popover 和 Tooltip 的组合方式适配为 `WorkbenchUI.tsx`，配合 FTW 主题。 |
| [Radix Primitives](https://www.radix-ui.com/primitives/docs/overview/introduction) | Dropdown Menu 2.1.25、Popover 1.1.23、Tooltip 1.2.16，MIT | 运行时提供菜单导航、定位、焦点恢复和 Escape 关闭。 |
| [Lucide React](https://lucide.dev/guide/packages/lucide-react) | 0.577.0，ISC，继承的 Feather 部分按 MIT | 导航、客户视图、表格、详情与工具栏图标。 |
| [TanStack Table](https://tanstack.com/table/latest/docs/guide/tables) | 沿用 9.3.1，MIT | 维护客户表格表头、选区及列宽状态。数据仍由 Core 按筛选、排序和分页请求返回。 |
| [React Query Builder](https://github.com/react-querybuilder/react-querybuilder) | 8.28.0，MIT | 管理高级筛选的字段、操作符、条件和嵌套分组。FTW 将规则转换为 Core 的筛选表达式。 |
| [TanStack Query](https://github.com/TanStack/query) | 5.108.3，MIT | 复用现有查询、变更和缓存更新机制，连接桌面工作区与 Core。 |

Twenty CRM 用于参考联系人身份、企业信息与记录活动的布局。AppFlowy 用于参考字段可见性、顺序、列宽和保存视图的交互。这两项都属于设计研究，无源代码移植或运行时依赖。 [Twenty 仓库及许可](https://github.com/twentyhq/twenty/blob/main/LICENSE) · [AppFlowy 数据库插件与许可](https://github.com/AppFlowy-IO/AppFlowy/tree/main/frontend/appflowy_flutter/lib/plugins/database)

版权和许可声明位于 [第三方 UI 许可](../../apps/desktop/public/THIRD_PARTY_UI.txt)，便携版构建会随程序附上。

## 1.1.9 选择控件

使用 npm `@radix-ui/react-select` 2.4.1（MIT，WorkOS）和 `cmdk` 1.1.1（MIT，Paco Coursey），结合现有 Radix Popover，实现统一单选、搜索、键盘与弹层交互。`Select.tsx` 保留原表单的选项值、清空选项、禁用及必填校验，不引入新的业务状态。版权声明随包保存在 `THIRD_PARTY_UI.txt`。
