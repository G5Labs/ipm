Class NcInstOrd.Upd.V3 Extends %IPM.General.Update.VersionBase
{

ClassMethod GetOrderedMethods() As %Library.DynamicArray
{
    return ["A"]
}

ClassMethod GetOrderedMethodsPrimaryOnly() As %Library.DynamicArray
{
    return []
}

ClassMethod A()
{
    set ^NcInstOrd("order") = $get(^NcInstOrd("order"))_$select($get(^NcInstOrd("order"))="":"",1:",")_"V3.A"
}

}
