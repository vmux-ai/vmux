#[path = "../build_git_env.rs"]
mod build_git_env;

fn main() {
    build_git_env::emit();
}
