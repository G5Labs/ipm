Class Iter7.Settings Extends %IPM.Repo.UniversalSettings
{
Parameter CONFIGURABLE = "Known";
Parameter Known = "Known";
ClassMethod GetValue(indx As %String = "") As %String
{
 set ^Iter7Reads=$increment(^Iter7Reads)
 return "observed"
}
}
