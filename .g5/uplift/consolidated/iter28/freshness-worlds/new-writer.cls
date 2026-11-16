Class IPM.FreshnessWriter {
ClassMethod RewriteSource() {
 do ##class(%IPM.Repo.UniversalSettings).SetValue("DefaultLogEntryLimit",999)
}
}
