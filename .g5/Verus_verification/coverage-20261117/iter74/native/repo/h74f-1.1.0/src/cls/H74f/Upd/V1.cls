Class H74f.Upd.V1 Extends %IPM.General.Update.VersionBase
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
    set ^IPMCov74("h74f","S1") = $increment(^IPMCov74("h74f","S1runs"))
    $$$ThrowStatus($$$ERROR($$$GeneralError, "h74f update step failure"))
}

}
