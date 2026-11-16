Include %IPM.Common
Class Iter23Controls.Hooks {
ClassMethod Fail() As %Status { quit $$$ERROR($$$GeneralError,"first"_$char(13)_"second"_$char(10)_"third"_$char(9)_"fourth") }
}
