const messages: Record<string, string> = {
  backup_mail_busy: '邮件发送、收取或审批正在进行，请稍后恢复。',
  backup_restore_failed: '无法恢复此备份，请检查文件及版本。',
  backup_create_failed: '创建一致性备份失败，请检查目录权限与空间。',
  backup_list_failed: '无法读取备份目录。',
  backup_inspect_failed: '备份校验失败。请核对 SQLite 文件、版本及配套资料文件。',
  csv_size_limit: '请选择不超过 512 KiB 的 UTF-8 CSV。',
  csv_row_limit: '每批最多 500 条，请拆成多个 CSV 文件。',
  csv_name_column_required: '需要「客户名称」或「name」这一列。',
  csv_invalid: '无法读取 CSV 表头。',
  csv_export_failed: '导出失败，请检查数据目录权限。',
  csv_export_fields_invalid: '请选择至少一个有效的导出字段。',
  database_unavailable: '数据服务暂时不可用。',
  invalid_search_keyword: '请输入 1–160 个字符的搜索词。',
  invalid_analytics_range: '请选择最近 7、30 或 90 天。',
  invalid_audit_filter: '请检查审计筛选条件，关键词最多 160 个字符。',
}

export function DataError({ error }: { error: unknown }) {
  const code = typeof error === 'object' && error && 'code' in error ? String(error.code) : String(error)
  return <p className="alert error" role="alert">{messages[code] ?? code}</p>
}
