Include %IPM.Common
Class Iter9.Seed {
ClassMethod OnlyAny() {
 set m=##class(%IPM.Storage.Module).NameOpen("iter9-step-b"),s=m.GetAddUpdateStep("Iter9.Steps.V1","A",0)
 do m.SeedUpdateStep(s)
}
}
