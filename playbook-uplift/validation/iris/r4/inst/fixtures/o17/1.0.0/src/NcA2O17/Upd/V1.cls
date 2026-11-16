Class NcA2O17.Upd.V1 Extends %IPM.General.Update.VersionBase
{

ClassMethod GetOrderedMethods() As %Library.DynamicArray
{
    return ["S1"]
}

ClassMethod S1()
{
    set ^NcA2O17("V1.S1") = $get(^NcA2O17("V1.S1")) + 1
}

}
