Class OrasTrustApp.Info
{

ClassMethod Content() As %String
{
    return "2.0.0"
}

ClassMethod Describe() As %String
{
    return "app " _ ..Content() _ " with dep " _ ##class(OrasTrustDep.Info).Version()
}

/// Invoked by the module's lifecycle, so tests can tell whether it ran
ClassMethod RecordConfigure()
{
    set ^OrasTrustApp("configured", ..Content()) = ""
}

}
