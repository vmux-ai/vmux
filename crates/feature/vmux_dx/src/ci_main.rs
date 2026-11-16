use vmux_ecs::cli::CliInvocation;

fn main() {
    let invocation: vmux_dx::Result<CliInvocation> =
        vmux_dx::CiPlugin::invocation(std::env::args_os().skip(1).collect());
    match invocation.and_then(vmux_dx::CiPlugin::run) {
        Ok(code) => std::process::exit(i32::from(code)),
        Err(error) => {
            eprintln!("vmux-ci: {error}");
            std::process::exit(1);
        }
    }
}
