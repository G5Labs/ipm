Class IPM.FreshnessWriter {
ClassMethod ChangeSupplier(m As %IPM.Storage.Module,n As %String) {
 set m.Repository=n
 do m.%Save()
}
}
