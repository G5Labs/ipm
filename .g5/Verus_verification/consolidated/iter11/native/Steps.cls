Class Iter11.Steps.V1 Extends %IPM.General.Update.VersionBase
{
ClassMethod GetOrderedMethods() As %DynamicArray { return ["Shared"] }
ClassMethod GetOrderedMethodsPrimaryOnly() As %DynamicArray { return [] }
ClassMethod Shared()
{
 set label=$get(^Iter11("label",$job),"unknown")
 set ^Iter11("tlevel",label)=$tlevel
 set count=$increment(^Iter11("count"))
 set ^Iter11("value")=1
 set ^Iter11("entered",label)=1
 for i=1:1:600 { quit:$get(^Iter11("release",label))  hang 0.05 }
 if '$get(^Iter11("release",label)) {set ^Iter11("timeout",label)=1}
}
}
