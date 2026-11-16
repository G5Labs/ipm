/// Calls the private relative-path converter of the real Test resource processor.
Class Iter4.RelPath Extends %IPM.ResourceProcessor.Test
{

ClassMethod Drive()
{
    set dir = "tests/unit"
    for rel = "tests/unit/SyncTest/Tests/Trivial.cls", "other/SyncTest/Tests/Trivial.cls", "tests/unit/SyncTest/readme.txt", "tests/unitX/A/B.cls" {
        do ##class(Iter4.Probe).Obs("C-"_rel, "class", ..RelPathToClassName(rel, dir))
    }
}

}
