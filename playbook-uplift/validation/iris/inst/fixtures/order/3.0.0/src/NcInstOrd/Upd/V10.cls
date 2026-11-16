Class NcInstOrd.Upd.V10 Extends %IPM.General.Update.VersionBase
{

ClassMethod GetOrderedMethods() As %Library.DynamicArray
{
    return ["A"]
}

ClassMethod GetOrderedMethodsPrimaryOnly() As %Library.DynamicArray
{
    return ["B"]
}

ClassMethod A()
{
    set ^NcInstOrd("order") = $get(^NcInstOrd("order"))_$select($get(^NcInstOrd("order"))="":"",1:",")_"V10.A"
}

ClassMethod B()
{
    set ^NcInstOrd("order") = $get(^NcInstOrd("order"))_$select($get(^NcInstOrd("order"))="":"",1:",")_"V10.B"
}

}
