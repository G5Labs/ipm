fn main() {
    println!("cargo:rerun-if-changed=../g5-api/src/db/migrations");
}
