Class IPM.FreshnessWriter {
ClassMethod RewriteStatus(r As %IPM.General.History,s As %Status) {
 set r.Success=s
 do r.%Save()
}
}
