Class IPM.FreshnessWriter {
ClassMethod RegisterOwnership(r As %IPM.Storage.ResourceReference,m As %IPM.Storage.Module) {
 set r.Module=m
 do r.%Save()
}
}
