Class NcInstM.Upd.V1 Extends %IPM.General.Update.VersionBase
{

ClassMethod GetOrderedMethods() As %Library.DynamicArray
{
    return ["S1"]
}

ClassMethod S1()
{
    set ^NcInstM("V1.S1") = $get(^NcInstM("V1.S1")) + 1
}

}
