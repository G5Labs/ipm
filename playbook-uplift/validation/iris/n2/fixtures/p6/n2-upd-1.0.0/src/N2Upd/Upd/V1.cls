Class N2Upd.Upd.V1 Extends %IPM.General.Update.VersionBase
{

ClassMethod GetOrderedMethods() As %Library.DynamicArray
{
    return ["S1"]
}

ClassMethod S1()
{
    set ^N2Upd("V1.S1") = $get(^N2Upd("V1.S1")) + 1
}

}
