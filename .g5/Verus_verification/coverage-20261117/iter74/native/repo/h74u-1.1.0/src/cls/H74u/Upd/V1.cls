Class H74u.Upd.V1 Extends %IPM.General.Update.VersionBase
{

ClassMethod GetOrderedMethods() As %Library.DynamicArray
{
    return ["S1"]
}

ClassMethod GetOrderedMethodsPrimaryOnly() As %Library.DynamicArray
{
    return []
}

ClassMethod S1()
{
    set ^IPMCov74("h74u","S1") = 1
    do ##class(IPMCov.Reader74).Mid("update", "update", "h74u")
}

}
