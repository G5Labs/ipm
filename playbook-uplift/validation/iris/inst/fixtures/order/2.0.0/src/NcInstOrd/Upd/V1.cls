Class NcInstOrd.Upd.V1 Extends %IPM.General.Update.VersionBase
{

ClassMethod GetOrderedMethods() As %Library.DynamicArray
{
    return ["S"]
}

ClassMethod GetOrderedMethodsPrimaryOnly() As %Library.DynamicArray
{
    return []
}

ClassMethod S()
{
    set ^NcInstOrd("order") = $get(^NcInstOrd("order"))_$select($get(^NcInstOrd("order"))="":"",1:",")_"V1.S"
}

}
