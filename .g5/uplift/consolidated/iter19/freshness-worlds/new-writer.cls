Class IPM.FreshnessWriter {
ClassMethod Replace(c As %IPM.Repo.Filesystem.Cache,dir As %String) {
 set c.SubDirectory=dir
 do c.%Save()
}
}
