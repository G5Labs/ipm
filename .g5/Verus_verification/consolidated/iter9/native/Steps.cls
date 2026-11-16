Class Iter9.Steps.V1 Extends %IPM.General.Update.VersionBase
{
ClassMethod GetOrderedMethods() As %DynamicArray { return ["A"] }
ClassMethod GetOrderedMethodsPrimaryOnly() As %DynamicArray { return ["P"] }
ClassMethod A() { set ^Iter9Ran=$get(^Iter9Ran)+1 }
ClassMethod P() { set ^Iter9Ran=$get(^Iter9Ran)+1 }
}
