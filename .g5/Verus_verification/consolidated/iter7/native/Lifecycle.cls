Class Iter7.Lifecycle Extends %IPM.Lifecycle.Module
{
ClassMethod LoadEnvironmentConfig(paths As %String)
{
 set ^Iter7EnvCalls=$increment(^Iter7EnvCalls)
 do ##super(paths)
}
}
