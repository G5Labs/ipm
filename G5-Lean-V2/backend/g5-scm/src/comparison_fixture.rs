//! One deterministic fast-import repository shared by comparison tests in each binary.
use std::{
    collections::BTreeMap,
    io::Write,
    process::{Command, Stdio},
    sync::OnceLock,
};
pub struct ComparisonFixture {
    pub directory: tempfile::TempDir,
    pub heads: BTreeMap<String, String>,
    pub scenarios: Vec<ComparisonScenario>,
}
pub struct ComparisonScenario {
    pub name: &'static str,
    pub source: &'static str,
    pub parent: &'static str,
    pub base: &'static str,
    pub merge_base: Option<&'static str>,
    pub defaults: Vec<&'static str>,
    pub paths: Vec<&'static str>,
}
pub fn comparison_fixture() -> &'static ComparisonFixture {
    static FIXTURE: OnceLock<ComparisonFixture> = OnceLock::new();
    FIXTURE.get_or_init(|| {
        let directory=tempfile::tempdir().unwrap();
        let init=Command::new("git").args(["init","--bare","--quiet"]).arg(directory.path()).output().unwrap(); assert!(init.status.success());
        let mut stream=String::new(); let mut marks=BTreeMap::new();
        let mut commit=|name:&str,parents:&[&str],files:&[(&str,&str)]| {
            let number=marks.len()+1; let message=format!("{name}\n");
            stream.push_str(&format!("commit refs/heads/{name}\nmark :{number}\nauthor Test <test@example.invalid> {} +0000\ncommitter Test <test@example.invalid> {} +0000\ndata {}\n{}",1700000000+number,1700000000+number,message.len(),message));
            for (i,parent) in parents.iter().enumerate() { stream.push_str(&format!("{} :{}\n",if i==0 {"from"} else {"merge"},marks[*parent])); }
            for (path,text) in files {stream.push_str(&format!("M 100644 inline {path}\ndata {}\n{text}\n",text.len()));}
            stream.push('\n'); marks.insert(name.to_string(),number);
        };
        commit("A",&[],&[("root.txt","root\n")]);
        commit("B",&["A"],&[("common.txt","base\n")]);
        commit("C",&["B"],&[("unrelated.txt","main\n")]);
        commit("D",&["C"],&[("parent.txt","advanced\n")]);
        commit("X1",&["B"],&[("feature.txt","feature\n")]);
        commit("X2",&["X1"],&[("tests.txt","tests\n")]);
        commit("R1",&["D"],&[("feature.txt","feature\n")]);
        commit("R2",&["R1"],&[("tests.txt","tests\n")]);
        commit("E",&["D"],&[("new-parent.txt","new\n")]);
        commit("Q1",&["E"],&[("feature.txt","feature\n")]);
        commit("Q2",&["Q1"],&[("tests.txt","tests\n")]);
        commit("Q3",&["Q2"],&[("later.txt","later\n")]);
        commit("M",&["X1","D"],&[("unrelated.txt","main\n"),("parent.txt","advanced\n")]);
        commit("S",&["B"],&[("common.txt","source\n")]);
        commit("P",&["B"],&[("common.txt","parent\n")]);
        commit("MC",&["S","P"],&[("common.txt","resolved\n")]);
        commit("CH",&["X1"],&[("child.txt","child\n")]);
        commit("CM",&["X1","CH"],&[("child.txt","child\n")]);
        commit("Z",&[],&[("root.txt","rewritten\n")]);
        commit("ZS",&["Z"],&[("feature.txt","feature\n")]);
        commit("AB",&["X1","D"],&[("unrelated.txt","main\n"),("parent.txt","advanced\n")]);
        commit("BA",&["D","X1"],&[("feature.txt","feature\n")]);
        commit("LANDED",&["D","R2"],&[("feature.txt","feature\n"),("tests.txt","tests\n")]);
        let mark_path=directory.path().join("marks");
        let mut child=Command::new("git").arg("--git-dir").arg(directory.path()).args(["fast-import","--quiet"]).arg(format!("--export-marks={}",mark_path.display())).stdin(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
        child.stdin.take().unwrap().write_all(stream.as_bytes()).unwrap();
        let result=child.wait_with_output().unwrap();assert!(result.status.success(),"{}",String::from_utf8_lossy(&result.stderr));
        let exported=std::fs::read_to_string(mark_path).unwrap().lines().map(|line| {let (a,b)=line.split_once(' ').unwrap();(a.trim_start_matches(':').parse::<usize>().unwrap(),b.to_string())}).collect::<BTreeMap<_,_>>();
        let mut heads:BTreeMap<String,String>=marks.into_iter().map(|(name,mark)|(name,exported[&mark].clone())).collect();
        heads.insert("MISSING".into(),"f".repeat(40));
        let scenario=|name,source,parent,base,merge_base,defaults,paths|ComparisonScenario{name,source,parent,base,merge_base,defaults,paths};
        let scenarios=vec![
            scenario("linear","X2","B","B",Some("B"),vec!["X2","X1"],vec!["feature.txt","tests.txt"]),
            scenario("rebase-834","R2","D","B",Some("D"),vec!["R2","R1"],vec!["feature.txt","tests.txt"]),
            scenario("repeated-rebases","Q3","E","B",Some("E"),vec!["Q3","Q2","Q1"],vec!["feature.txt","later.txt","tests.txt"]),
            scenario("parent-merge","M","D","B",Some("D"),vec!["M","X1"],vec!["feature.txt"]),
            scenario("conflict-resolution","MC","P","B",Some("P"),vec!["MC","S"],vec!["common.txt"]),
            scenario("child-merge","CM","D","B",Some("B"),vec!["CM","X1"],vec!["child.txt","feature.txt"]),
            scenario("landed-merge-frozen","R2","D","B",Some("D"),vec!["R2","R1"],vec!["feature.txt","tests.txt"]),
            scenario("landed-rebase-frozen","R2","D","B",Some("D"),vec!["R2","R1"],vec!["feature.txt","tests.txt"]),
            scenario("rewritten-parent","ZS","Z","B",Some("Z"),vec!["ZS"],vec!["feature.txt"]),
            scenario("empty","D","D","B",Some("D"),vec![],vec![]),
            scenario("missing-object","MISSING","D","B",None,vec![],vec![]),
            scenario("ambiguous","AB","BA","B",None,vec![],vec![]),
        ];
        ComparisonFixture{directory,heads,scenarios}
    })
}
