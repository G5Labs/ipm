// IPM 571e04a1: input-driven translation, not a verified ObjectScript body.
// ObjectScript numeric conversion is restricted to unsigned decimal strings here.
// ASCII collation, not arbitrary IRIS locale collation, is represented.
use vstd::prelude::*;
verus! {
pub type Text = Seq<char>;
pub struct Fields { pub major: Text, pub minor: Text, pub patch: Text, pub pre: Text, pub build: Text }
pub struct Version { pub major: nat, pub minor: nat, pub patch: nat, pub pre: Text, pub build: Text }
pub enum Op { Eq, Lt, Le, Gt, Ge }
pub struct Cmp { pub op: Op, pub v: Version }
pub type Range = Seq<Cmp>;
pub enum Intersection { Good(Range), Empty, ExactConflict, NoComparators }

pub open spec fn first(s: Text, c: char, i: nat) -> nat
    decreases s.len() - i
{ if i >= s.len() || s[i as int] == c { i } else { first(s,c,i+1) } }
pub open spec fn before(s: Text,c: char) -> Text { s.take(first(s,c,0) as int) }
pub open spec fn after(s: Text,c: char) -> Text {
    let i = first(s,c,0); if i >= s.len() { seq![] } else { s.skip((i+1) as int) }
}
pub open spec fn part(s: Text,c: char,n: nat) -> Text
    decreases n
{ if n == 0 { before(s,c) } else { part(after(s,c),c,(n-1) as nat) } }
pub open spec fn n_parts(s: Text,c: char,i: nat) -> nat
    decreases s.len() - i
{ if i >= s.len() { 1 } else { (if s[i as int] == c {1nat} else {0nat}) + n_parts(s,c,i+1) } }
pub open spec fn parse(s: Text) -> Fields {
    let nd = n_parts(before(s,'-'),'.',0);
    Fields { major: before(before(part(s,'.',0),'-'),'+'),
        minor: if nd > 1 { before(before(part(s,'.',1),'-'),'+') } else { seq!['0'] },
        patch: if nd > 2 { before(before(part(s,'.',2),'-'),'+') } else { seq!['0'] },
        pre: before(after(s,'-'),'+'),
        build: if first(s,'+',0) < s.len() { part(s,'+',1) } else if nd > 3 { part(s,'.',3) } else { seq![] } }
}
pub open spec fn digit(c: char) -> bool { '0' <= c && c <= '9' }
pub open spec fn decimal(s: Text,i: nat) -> bool
    decreases s.len()-i
{ i >= s.len() || (digit(s[i as int]) && decimal(s,i+1)) }
pub open spec fn number(s: Text,i: nat,acc: nat) -> nat
    decreases s.len()-i
{ if i >= s.len() || !digit(s[i as int]) { acc } else { number(s,i+1,acc*10+(s[i as int] as nat-'0' as nat) as nat) } }
pub open spec fn canonical(s: Text) -> bool { s.len()>0 && decimal(s,0) && (s.len()==1 || s[0]!='0') }
pub open spec fn id_char(c: char) -> bool { digit(c) || ('A'<=c && c<='Z') || ('a'<=c && c<='z') || c=='-' }
pub open spec fn identifiers(s: Text,i: nat) -> bool
    decreases s.len()-i
{ i >= s.len() || ((id_char(s[i as int]) || (s[i as int]=='.' && i>0 && i+1<s.len() && s[(i-1) as int]!='.')) && identifiers(s,i+1)) }
pub open spec fn leading_zero_ident(s: Text) -> bool
    decreases s.len()
{ if s.len()==0 { false } else {
    let p=before(s,'.'); (p.len()>1 && p[0]=='0' && decimal(p,0)) ||
        (first(s,'.',0)<s.len() && leading_zero_ident(after(s,'.')))
} }
pub open spec fn validate(f: Fields) -> bool {
    canonical(f.major) && canonical(f.minor) && canonical(f.patch) && identifiers(f.pre,0) && identifiers(f.build,0) && !leading_zero_ident(f.pre)
}
pub open spec fn version(f: Fields) -> Version { Version {major:number(f.major,0,0),minor:number(f.minor,0,0),patch:number(f.patch,0,0),pre:f.pre,build:f.build} }
pub open spec fn v(s: Text) -> Version { version(parse(s)) }
pub open spec fn fields_string(f: Fields) -> Text {
    f.major + seq!['.'] + f.minor + (if f.patch.len()==0 {seq![]} else {seq!['.']+f.patch})
      + (if f.pre.len()==0 {seq![]} else {seq!['-']+f.pre}) + (if f.build.len()==0 {seq![]} else {seq!['+']+f.build})
}
pub open spec fn lower(c: char) -> char { if 'A'<=c && c<='Z' { (c as u32+32) as char } else {c} }
pub open spec fn snapshot(f: Version) -> bool { f.build.map_values(|c:char| lower(c)) == seq!['s','n','a','p','s','h','o','t'] }
pub open spec fn same_core(a: Version,b: Version) -> bool { a.major==b.major && a.minor==b.minor && a.patch==b.patch }
pub open spec fn core_gt(a: Version,b: Version) -> bool { a.major>b.major || (a.major==b.major && a.minor>b.minor) || (a.major==b.major && a.minor==b.minor && a.patch>b.patch) }
pub open spec fn lex_gt(a: Text,b: Text,i: nat) -> bool
    decreases a.len()-i
{ if i >= a.len() { false } else if i >= b.len() {true} else if a[i as int]==b[i as int] {lex_gt(a,b,i+1)} else {a[i as int]>b[i as int]} }
pub open spec fn canon_idnum(s:Text) -> bool {
    canonical(s) || (s.len()>1 && s[0]=='-' && canonical(s.skip(1)) && number(s.skip(1),0,0)>0)
}
pub open spec fn signed_number(s:Text) -> int {if s.len()>0 && s[0]=='-' {-(number(s.skip(1),0,0) as int)} else {number(s,0,0) as int}}
pub open spec fn collates(a: Text,b: Text) -> bool {
    if canon_idnum(a) && canon_idnum(b) {signed_number(a)>signed_number(b)} else if canon_idnum(a) {false} else if canon_idnum(b) {true} else {lex_gt(a,b,0)}
}
pub open spec fn compare_dot(a: Text,b: Text) -> bool
    decreases a.len()
{ let ap=before(a,'.'); let bp=before(b,'.');
    if ap!=bp {collates(ap,bp)} else if first(a,'.',0)>=a.len() {false} else if first(b,'.',0)>=b.len() {true} else {compare_dot(after(a,'.'),after(b,'.'))}
}
pub open spec fn follows(a: Version,b: Version,post: bool) -> bool {
    if post {
        if core_gt(a,b) {true} else if a.major<b.major || a.minor<b.minor || a.patch<b.patch {false}
        else if a.pre.len()==0 {false} else if b.pre.len()==0 {true} else {compare_dot(a.pre,b.pre)}
    } else if a.pre.len()>0 || b.pre.len()>0 {
        if !same_core(a,b) { if a.pre.len()>0 && b.pre.len()>0 {false} else {core_gt(a,b)} }
        else if a.pre==b.pre {snapshot(b) && !snapshot(a)}
        else if a.pre.len()>0 && b.pre.len()==0 {false}
        else if a.pre.len()==0 && b.pre.len()>0 {true} else {compare_dot(a.pre,b.pre)}
    } else if same_core(a,b) {snapshot(b) && !snapshot(a)} else {core_gt(a,b)}
}
pub open spec fn drop_build(a: Version) -> Version {Version{build:seq![],..a}}
pub open spec fn cmp_holds(c: Cmp,a: Version,post: bool) -> bool {
    let equal = same_core(a,c.v) && (c.v.build.len()==0 || a.build==c.v.build) && ((post && !(c.op is Eq)) || a.pre==c.v.pre);
    match c.op { Op::Eq => equal, Op::Ge => equal || follows(drop_build(a),drop_build(c.v),post), Op::Le => equal || follows(drop_build(c.v),drop_build(a),post),
        Op::Gt => follows(drop_build(a),drop_build(c.v),post), Op::Lt => follows(drop_build(c.v),drop_build(a),post) }
}
pub open spec fn range_holds(r: Range,a: Version,post: bool,i: nat) -> bool
    decreases r.len()-i
{ i>=r.len() || (cmp_holds(r[i as int],a,post) && range_holds(r,a,post,i+1)) }
pub open spec fn cv(ma:nat,mi:nat,pa:nat,pre:Text) -> Version {Version{major:ma,minor:mi,patch:pa,pre,build:seq![]}}
pub open spec fn strip_prefix(s: Text) -> Text
    decreases s.len()
{ if s.len()>0 && (s[0]=='<' || s[0]=='>' || s[0]=='=' || s[0]=='^' || s[0]=='~' || s[0]==' ' || s[0]=='\t') {strip_prefix(s.skip(1))} else {s} }
pub open spec fn operator(s: Text) -> Op {
    if s.len()>0 && s[0]=='<' {if s.len()>1 && s[1]=='=' {Op::Le} else {Op::Lt}}
    else if s.len()>0 && s[0]=='>' {if s.len()>1 && s[1]=='=' {Op::Ge} else {Op::Gt}} else {Op::Eq}
}
pub open spec fn year_exact(s: Text) -> bool {
    s.len()==6 && s[0]=='2' && s[1]=='0' && digit(s[2]) && digit(s[3]) && s[4]=='.' && digit(s[5])
}
pub open spec fn wildcard(s: Text) -> bool {s.len()==0 || s==seq!['x'] || s==seq!['X'] || s==seq!['*']}
// FromString's partial-version branch discards the operator before expanding x ranges.
pub open spec fn expand(s: Text) -> Range {
    let raw=strip_prefix(s); let f=parse(raw); let a=version(f); let count=n_parts(before(before(raw,'-'),'+'),'.',0);
    let minor=part(raw,'.',1); let patch=part(raw,'.',2);
    let partial=count<3 || wildcard(part(raw,'.',0)) || wildcard(minor) || wildcard(patch);
    if raw.len()==0 || raw==seq!['*'] {seq![]}
    else if year_exact(raw) {seq![Cmp{op:operator(s),v:a}]}
    else if partial {
        if wildcard(minor) {seq![Cmp{op:Op::Ge,v:cv(a.major,0,0,seq![])},Cmp{op:Op::Lt,v:cv(a.major+1,0,0,seq!['0'])}]}
        else {seq![Cmp{op:Op::Ge,v:cv(a.major,a.minor,0,seq![])},Cmp{op:Op::Lt,v:cv(a.major,a.minor+1,0,seq!['0'])}]}
    } else if s.len()>0 && s[0]=='~' {
        seq![Cmp{op:Op::Ge,v:a},Cmp{op:Op::Lt,v:cv(a.major,a.minor+1,0,seq!['0'])}]
    } else if s.len()>0 && s[0]=='^' {
        seq![Cmp{op:Op::Ge,v:a},Cmp{op:Op::Lt,v:if a.major==0 {if a.minor==0 {cv(0,0,a.patch+1,seq!['0'])} else {cv(0,a.minor+1,0,seq!['0'])}} else {cv(a.major+1,0,0,seq!['0'])}}]
    } else {seq![Cmp{op:operator(s),v:a}]}
}
pub open spec fn group(r: Range,which: nat,i: nat) -> Range
    decreases r.len()-i
{ if i>=r.len() {seq![]} else {
    let c=r[i as int]; let has=if which==0 {c.op is Gt || c.op is Ge} else if which==1 {c.op is Lt || c.op is Le} else {c.op is Eq};
    (if has {seq![c]} else {seq![]}) + group(r,which,i+1)
} }
pub open spec fn compare_versions(a: Cmp,b: Cmp) -> int {
    if core_gt(a.v,b.v) {1} else if core_gt(b.v,a.v) {-1}
    else if (a.op is Gt && b.op is Ge) || (a.op is Le && b.op is Lt) {1}
    else if (a.op is Ge && b.op is Gt) || (a.op is Lt && b.op is Le) {-1} else {0}
}
pub open spec fn best(r: Range,lower: bool,i: nat,cur: Option<Cmp>) -> Option<Cmp>
    decreases r.len()-i
{ if i>=r.len() {cur} else {
    let c=r[i as int]; let n=match cur {None=>Some(c),Some(old)=>if (lower && compare_versions(old,c)<0) || (!lower && compare_versions(old,c)>0) {Some(c)} else {cur}};
    best(r,lower,i+1,n)
} }
pub open spec fn valid_intersection(lo: Cmp,up: Cmp,post: bool) -> bool {
    if follows(drop_build(up.v),drop_build(lo.v),post) {true} else if follows(drop_build(lo.v),drop_build(up.v),post) {false}
    else {!(lo.op is Gt || up.op is Lt)}
}
pub open spec fn exact_agree(r: Range,v: Version,i: nat) -> bool
    decreases r.len()-i
{ i>=r.len() || (same_core(r[i as int].v,v) && r[i as int].v.pre==v.pre && exact_agree(r,v,i+1)) }
pub open spec fn intersect(a: Range,b: Range,post: bool) -> Intersection {
    let r=a+b; let lo=group(r,0,0); let up=group(r,1,0); let ex=group(r,2,0);
    if ex.len()>0 {
        if !exact_agree(ex,ex[0].v,0) {Intersection::ExactConflict}
        else if !range_holds(lo+up,drop_build(ex[0].v),post,0) {Intersection::Empty}
        else {Intersection::Good(seq![ex[0]])}
    } else {
        let l=best(lo,true,0,None); let u=best(up,false,0,None);
        match (l,u) {(Some(x),Some(y))=>if valid_intersection(x,y,post) {Intersection::Good(seq![x,y])} else {Intersection::Empty},
            (Some(x),None)=>Intersection::Good(seq![x]),(None,Some(y))=>Intersection::Good(seq![y]),(None,None)=>Intersection::NoComparators}
    }
}
pub open spec fn chosen(r: Range,offered: Seq<Version>,post: bool,i:nat) -> Option<Version>
    decreases offered.len()-i
{ if i>=offered.len() {None} else if range_holds(r,offered[i as int],post,0) {Some(offered[i as int])} else {chosen(r,offered,post,i+1)} }
pub open spec fn load_result(old:Version,new:Version,post:bool) -> Version {if follows(old,new,post) {old} else {new}}
pub open spec fn load_refused(old:Version,new:Version,post:bool) -> bool {follows(old,new,post)}
// Resolver accumulation order is current requirement first; the loader rechecks parent-name order.
pub open spec fn selected_holds(check:Range,selection:Option<Version>) -> bool {
    match selection {Some(x)=>range_holds(check,x,false,0),None=>false}
}
pub open spec fn diamond(first:Range,second:Range,offered:Seq<Version>) -> bool {
    let search=intersect(second,first,false); let check=intersect(first,second,false);
    if search is Good && check is Good {selected_holds(good_range(check),chosen(good_range(search),offered,false,0))} else {false}
}
pub open spec fn and_error(why:Intersection,inline:bool) -> Text {
    match why {Intersection::Empty=>if inline {seq!['N','o',' ','o','v','e','r','l','a','p','p','i','n','g',' ','v','e','r','s','i','o','n','s',' ','i','n',' ','&','&',' ','o','p','e','r','a','t','i','o','n',' ','f','o','r',' ','%','1',' ','a','n','d',' ','%','2']} else {seq!['N','o',' ','o','v','e','r','l','a','p','p','i','n','g',' ','v','e','r','s','i','o','n','s',' ','i','n',' ','A','N','D',' ','o','p','e','r','a','t','i','o','n']},
    Intersection::ExactConflict=>seq!['E','x','a','c','t',' ','v','e','r','s','i','o','n',' ','m','a','t','c','h','e','s',' ','d','o',' ','n','o','t',' ','a','g','r','e','e'],Intersection::NoComparators=>seq!['C','a','n','n','o','t',' ','m','e','r','g','e',' ','r','a','n','g','e','s'],_=>seq![]}
}

pub open spec fn nat_text(n:nat) -> Text
    decreases n
{ if n<10 {seq![(n+'0' as nat) as char]} else {nat_text(n/10)+seq![((n%10)+'0' as nat) as char]} }
pub open spec fn version_text(a:Version) -> Text {
    nat_text(a.major)+seq!['.']+nat_text(a.minor)+seq!['.']+nat_text(a.patch)
        +(if a.pre.len()==0 {seq![]} else {seq!['-']+a.pre})+(if a.build.len()==0 {seq![]} else {seq!['+']+a.build})
}
pub open spec fn cmp_text(c:Cmp) -> Text {
    (match c.op {Op::Eq=>seq!['='],Op::Lt=>seq!['<'],Op::Le=>seq!['<','='],Op::Gt=>seq!['>'],Op::Ge=>seq!['>','=']})+version_text(c.v)
}
pub open spec fn range_text(r:Range,i:nat) -> Text
    decreases r.len()-i
{ if i>=r.len() {seq![]} else {cmp_text(r[i as int])+(if i+1<r.len() {seq![' ']} else {seq![]})+range_text(r,i+1)} }
pub type Expr = Seq<Range>;
pub open spec fn expr_satisfied(e:Expr,a:Version,post:bool,i:nat) -> bool
    decreases e.len()-i
{ e.len()==0 || (i<e.len() && (range_holds(e[i as int],a,post,0) || expr_satisfied(e,a,post,i+1))) }
pub open spec fn expr_text(e:Expr,i:nat) -> Text
    decreases e.len()-i
{ if i>=e.len() {seq![]} else {
    (if e.len()>1 {seq!['(']+range_text(e[i as int],0)+seq![')']} else {range_text(e[i as int],0)})
        +(if i+1<e.len() {seq![' ','|','|',' ']} else {seq![]})+expr_text(e,i+1)
} }
pub open spec fn and_row(a:Range,b:Expr,post:bool,i:nat) -> Expr
    decreases b.len()-i
{ if i>=b.len() {seq![]} else {
    (match intersect(a,b[i as int],post) {Intersection::Good(r)=>seq![r],_=>seq![]})+and_row(a,b,post,i+1)
} }
pub open spec fn and_pairs(a:Expr,b:Expr,post:bool,i:nat) -> Expr
    decreases a.len()-i
{ if i>=a.len() {seq![]} else {and_row(a[i as int],b,post,0)+and_pairs(a,b,post,i+1)} }
// Error preservation is explicit: exact-conflict errors do not become an empty option.
pub open spec fn exact_conflict(a:Expr,b:Expr,i:nat,j:nat) -> bool
    decreases a.len()-i,b.len()-j
{ if i>=a.len() {false} else if j>=b.len() {exact_conflict(a,b,i+1,0)}
    else {intersect(a[i as int],b[j as int],false) is ExactConflict || exact_conflict(a,b,i,j+1)} }
pub enum ExprAnd {Good(Expr),ExactConflict,NoOverlap}
pub open spec fn expr_and(a:Expr,b:Expr,other_nonempty:bool,post:bool) -> ExprAnd {
    if !other_nonempty || b.len()==0 {ExprAnd::Good(a)} else if a.len()==0 {ExprAnd::Good(b)}
    else if exact_conflict(a,b,0,0) {ExprAnd::ExactConflict} else {
        let merged=and_pairs(a,b,post,0);if merged.len()==0 {ExprAnd::NoOverlap} else {ExprAnd::Good(merged)}
    }
}

// Lexical assembly is distinct from range semantics, so errors are observable inputs/results.
pub open spec fn remove_parens(s:Text,i:nat) -> Text
    decreases s.len()-i
{ if i>=s.len() {seq![]} else {(if s[i as int]=='(' || s[i as int]==')' {seq![]} else {seq![s[i as int]]})+remove_parens(s,i+1)} }
pub open spec fn trim_left(s:Text) -> Text
    decreases s.len()
{ if s.len()>0 && (s[0]==' ' || s[0]=='\t') {trim_left(s.skip(1))} else {s} }
pub open spec fn trim_right(s:Text) -> Text
    decreases s.len()
{ if s.len()>0 && (s[s.len()-1]==' ' || s[s.len()-1]=='\t') {trim_right(s.take(s.len()-1))} else {s} }
pub open spec fn trim(s:Text) -> Text {trim_right(trim_left(s))}
pub open spec fn split_char(s:Text,c:char) -> Seq<Text>
    decreases s.len()
{ let pos=first(s,c,0); if pos>=s.len() {seq![s]} else {seq![s.take(pos as int)]+split_char(s.skip((pos+1) as int),c)} }
pub open spec fn pair_pos(s:Text,c:char,i:nat) -> nat
    decreases s.len()-i
{ if i+1>=s.len() {s.len()} else if s[i as int]==c && s[(i+1) as int]==c {i} else {pair_pos(s,c,i+1)} }
pub open spec fn split_pair(s:Text,c:char) -> Seq<Text>
    decreases s.len()
{ let pos=pair_pos(s,c,0);if pos+1>=s.len() {seq![s]} else {seq![s.take(pos as int)]+split_pair(s.skip((pos+2) as int),c)} }
pub open spec fn hyphen(a:Text,b:Text) -> Range {
    let lo=v(a);let hi=v(b);let n=n_parts(b,'.',0);
    seq![Cmp{op:Op::Ge,v:lo},Cmp{op:if n<3 {Op::Lt} else {Op::Le},v:if n==1 {cv(hi.major+1,0,0,seq![])} else if n==2 {cv(hi.major,hi.minor+1,0,seq![])} else {hi}}]
}
pub open spec fn range_tokens(parts:Seq<Text>,i:nat) -> Range
    decreases parts.len()-i
{ if i>=parts.len() {seq![]} else if i+2<parts.len() && parts[(i+1) as int]==seq!['-'] {
    hyphen(parts[i as int],parts[(i+2) as int])+range_tokens(parts,i+3)
} else {expand(parts[i as int])+range_tokens(parts,i+1)} }
pub open spec fn parse_range(raw:Text) -> Range {range_tokens(split_char(trim(raw),' '),0)}
pub enum Expression {Parsed(Expr),Mixed,NoOverlapInline,ExactConflict,NoComparators}
pub open spec fn union_tokens(parts:Seq<Text>,i:nat) -> Expr
    decreases parts.len()-i
{ if i>=parts.len() {seq![]} else {
    let r=parse_range(trim(remove_parens(parts[i as int],0)));
    (if r.len()==0 {seq![]} else {seq![r]})+union_tokens(parts,i+1)
} }
pub open spec fn inline_tokens(parts:Seq<Text>,i:nat,acc:Option<Range>) -> Expression
    decreases parts.len()-i
{ if i>=parts.len() {Expression::Parsed(match acc {Some(r)=>seq![r],None=>seq![]})} else {
    let r=parse_range(trim(remove_parens(parts[i as int],0)));
    if r.len()==0 {inline_tokens(parts,i+1,acc)} else {match acc {
        None=>inline_tokens(parts,i+1,Some(r)),Some(old)=>match intersect(old,r,false) {
            Intersection::Good(m)=>inline_tokens(parts,i+1,Some(m)),Intersection::Empty=>Expression::NoOverlapInline,
            Intersection::ExactConflict=>Expression::ExactConflict,Intersection::NoComparators=>Expression::NoComparators
        }
    }}
} }
pub open spec fn expression_set(raw:Text) -> Expression {
    let has_and=pair_pos(raw,'&',0)<raw.len();let has_or=pair_pos(raw,'|',0)<raw.len();
    if has_and && has_or {Expression::Mixed} else if has_and {inline_tokens(split_pair(raw,'&'),0,None)}
    else {Expression::Parsed(union_tokens(split_pair(raw,'|'),0))}
}
pub open spec fn parsed(e:Expression) -> Expr {match e {Expression::Parsed(r)=>r,_=>seq![]}}
pub proof fn diamond_composition(first:Range,second:Range,offered:Seq<Version>,selected:Version)
    requires intersect(second,first,false) is Good,intersect(first,second,false) is Good,
        chosen(good_range(intersect(second,first,false)),offered,false,0)==Some(selected)
    ensures diamond(first,second,offered)==range_holds(good_range(intersect(first,second,false)),selected,false,0)
{}

pub proof fn prereleases_different_core_incomparable(a:Version,b:Version)
    requires a.pre.len()>0,b.pre.len()>0,!same_core(a,b)
    ensures !follows(a,b,false),!follows(b,a,false)
{}
pub proof fn release_downgrade_refused(a:Version,b:Version)
    requires a.pre.len()==0,b.pre.len()==0,core_gt(a,b)
    ensures load_refused(a,b,false),load_result(a,b,false)==a
{}
pub proof fn first_bound_tie_wins(a:Cmp,b:Cmp,lower:bool)
    requires compare_versions(a,b)==0
    ensures best(seq![a,b],lower,0,None)==Some(a)
{ reveal_with_fuel(best,3); }
pub proof fn exact_range_bridge(r:Range,a:Version,b:Version)
    requires a==b
    ensures range_holds(r,a,false,0)==range_holds(r,b,false,0)
{}
pub open spec fn good_range(i:Intersection) -> Range {match i {Intersection::Good(r)=>r,_=>seq![]}}
pub open spec fn native_offered() -> Seq<Version> {seq![cv(2,0,0,seq!['b','e','t','a','.','1']),cv(1,5,0,seq![])]}
pub open spec fn r_caret_one() -> Range {seq![Cmp{op:Op::Ge,v:cv(1,0,0,seq![])},Cmp{op:Op::Lt,v:cv(2,0,0,seq!['0'])}]}
pub open spec fn r_lt_two() -> Range {seq![Cmp{op:Op::Lt,v:cv(2,0,0,seq![])}]}
pub proof fn witnessed_product_distinctions() {
    assert(intersect(r_caret_one(),r_lt_two(),false) is Good) by(compute);
    assert(!range_holds(good_range(intersect(r_caret_one(),r_lt_two(),false)),cv(2,0,0,seq!['b','e','t','a','.','1']),false,0)) by(compute);
    assert(range_holds(good_range(intersect(r_lt_two(),r_caret_one(),false)),cv(2,0,0,seq!['b','e','t','a','.','1']),false,0)) by(compute);
    assert(parse(seq!['1','.','2','.','3','+','b','u','i','l','d','-','x']).pre==seq!['x']) by (compute);
    assert(parse(seq!['1','.','2','.','3','+','b','u','i','l','d','-','x']).build==seq!['b','u','i','l','d','-','x']) by (compute);
    assert(range_holds(expand(seq!['<','2']),v(seq!['2','.','5','.','0']),false,0)) by (compute);
    assert(!range_holds(expand(seq!['>','=','1','.','2']),v(seq!['1','.','3','.','0']),false,0)) by (compute);
    assert(range_holds(expand(seq!['^','1','.','2']),v(seq!['1','.','2','.','3']),false,0)) by (compute);
    assert(!range_holds(expand(seq!['^','1','.','2']),v(seq!['1','.','3','.','0']),false,0)) by (compute);
    assert(!load_refused(v(seq!['2','.','0','.','0','-','b','e','t','a']),v(seq!['1','.','0','.','0','-','r','c']),false)) by (compute);
    assert(load_refused(v(seq!['2','.','0','.','0']),v(seq!['1','.','0','.','0']),false)) by (compute);
    assert(chosen(good_range(intersect(r_lt_two(),r_caret_one(),false)),native_offered(),false,0) is Some) by(compute);
    assert(chosen(good_range(intersect(r_lt_two(),r_caret_one(),false)),native_offered(),false,0).unwrap().major==2) by(compute);
}
}
fn main() {}
