Class IPM.FreshnessWriter {
ClassMethod ChangeCachedVersion(r As %IPM.Repo.Filesystem.Cache,v As %String) {
 set r.VersionString=v
 do r.%Save()
}
}
