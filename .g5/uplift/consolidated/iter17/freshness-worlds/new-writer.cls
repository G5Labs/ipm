Class IPM.FreshnessWriter {
ClassMethod Export(path,id) {
 do ##class(%IPM.Test.JsonOutput).ToFile(path,id)
}
}
