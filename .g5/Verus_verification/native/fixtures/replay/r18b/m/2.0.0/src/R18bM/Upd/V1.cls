Class R18bM.Upd.V1 Extends %IPM.General.Update.VersionBase
{

ClassMethod GetOrderedMethodsPrimaryOnly() As %Library.DynamicArray
{
    return []
}

ClassMethod GetOrderedMethods() As %Library.DynamicArray
{
    return ["StepA"]
}

ClassMethod StepA()
{
    write !, "R18bM.Upd.V1:StepA"
}

}
