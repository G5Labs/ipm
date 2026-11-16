Class IPM.FreshnessWriter {
ClassMethod Replace(m As %IPM.Storage.Module) {
 set m.Repository="alpha"
 do m.%Save()
}
}
