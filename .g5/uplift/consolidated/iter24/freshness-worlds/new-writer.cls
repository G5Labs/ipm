Class IPM.FreshnessWriter {
ClassMethod RewriteDependency(m As %IPM.Storage.Module) {
 set m.VersionString="1.0.0+snapshot"
 do m.%Save()
}
}
