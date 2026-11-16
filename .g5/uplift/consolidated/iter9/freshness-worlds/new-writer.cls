Class IPM.FreshnessWriter {
ClassMethod Register(m,s) {
 set s.HomeModule=m
 do s.%Save()
}
}
