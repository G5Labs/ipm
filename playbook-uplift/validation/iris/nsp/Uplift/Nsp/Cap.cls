/// Output capture that also works while IPM switches into a namespace without IPM
/// (IPM's own BeginCaptureOutput redirects to ^%IPM.Utils.Module.1, which such a namespace cannot see).
/// Same technique as %IPM.Utils.Module; this class is mapped into every scratch namespace.
Class Uplift.Nsp.Cap
{

ClassMethod Begin() As %Status [ ProcedureBlock = 0 ]
{
    kill ^||ncCapture
    use $io::("^"_$zname)
    do $zutil(82,12,1)
    quit 1

rstr(sz,to) [rt] public {
    new rt set vr="rt"
    set rd=$zutil(82,12,0)
    set:$data(sz) vr=vr_"#"_sz set:$data(to) vr=vr_":"_to
    read @vr
    do:$data(to) $zutil(96,4,$test)
    do $zutil(82,12,rd)
    quit rt
  }
wchr(s)public { do write($char(s)) }
wff() public { do write($char(12)) }
wnl() public {
    if '$data(^||ncCapture(0)) set ^||ncCapture(0)=1,^(1)=""
    set ^||ncCapture($increment(^||ncCapture(0)))=""
  }
wstr(s) public { do write(s) }
wtab(s) public { do write($justify("",s-$x)) }
write(s) public {
    set lf=$find(s,$char(10))
    while lf {
        do write($extract(s,1,lf-2)),wnl()
        set s=$extract(s,lf,*)
        set lf=$find(s,$char(10))
    }
    set lno=$get(^||ncCapture(0)) set:lno="" lno=1,^||ncCapture(0)=1
    set ^||ncCapture(lno)=$get(^||ncCapture(lno))_$translate(s,$char(13))
  }
}

ClassMethod End(Output pText)
{
    kill pText
    do $zutil(82,12,0)
    use $io::("")
    set last = $get(^||ncCapture(0), 0)
    for k=1:1:last {
        set line = $get(^||ncCapture(k))
        continue:(k=last)&&(line="")
        set pText($increment(pText)) = line
    }
    kill ^||ncCapture
}

}
