Class NcA2O17.Upd.V2 Extends %IPM.General.Update.VersionBase
{

ClassMethod GetOrderedMethods() As %Library.DynamicArray
{
    return ["A"]
}

ClassMethod A()
{
    set ^NcA2O17("V2.A") = $get(^NcA2O17("V2.A")) + 1
}

}
