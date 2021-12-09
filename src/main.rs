use std::str::FromStr;
use structopt::StructOpt;
use swayipc::Connection;

#[derive(Debug)]
enum Workspace {
    Prev,
    Next,
    Num(i32),
}

impl FromStr for Workspace {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "prev" => Ok(Self::Prev),
            "next" => Ok(Self::Next),
            s => Ok(Self::Num(
                s.parse::<i32>()
                    .map_err(|_| format!("Can't parse {} as Command", s))?,
            )),
        }
    }
}

impl std::fmt::Display for Workspace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Workspace::Next => write!(f, "next"),
            Workspace::Prev => write!(f, "prev"),
            Workspace::Num(n) => write!(f, "number {}", n),
        }
    }
}

fn get_active_workspace_name(wm: &mut Connection) -> Option<String> {
    let outputs = wm.get_outputs().unwrap();
    let current_output = outputs.iter().find(|o| o.active).unwrap();
    current_output.current_workspace.clone()
}

fn pick_destination(wm: &mut Connection, opt: Opt) -> Workspace {
    // TODO: filter the ones on this output?
    // (Think multiscreen)
    let all_workspaces = wm.get_workspaces().unwrap();
    let max_workspace_num = all_workspaces.iter().map(|w| w.num).max().unwrap();
    let min_workspace_num = all_workspaces.iter().map(|w| w.num).min().unwrap();
    let current_workspace = all_workspaces
        .iter()
        .find(|w| Some(&w.name) == get_active_workspace_name(wm).as_ref())
        .unwrap();

    match opt.to {
        Workspace::Next => {
            match opt.command {
                Command::MoveToWorkspace => {
                    // If at max workspace and workspace not empty, create one extra workspace
                    // If the workspace is empty, loop back to the front instead
                    if current_workspace.num == max_workspace_num
                        && current_workspace.representation != ""
                        && current_workspace.num < 10
                    {
                        Workspace::Num(current_workspace.num + 1)
                    } else {
                        // Skip gaps when cycling between workspaces so the
                        // behaviour can be predictable
                        opt.to
                    }
                }
                Command::MoveContainerToWorkspace => {
                    if current_workspace.num == max_workspace_num && current_workspace.num >= 10 {
                        Workspace::Num(1)
                    } else {
                        // When moving a window, we want to occupy any gap between workspaces
                        // If we hit the max workspace, we want to go beyond it
                        Workspace::Num(current_workspace.num + 1)
                    }
                }
            }
        }
        Workspace::Prev => {
            match opt.command {
                Command::MoveToWorkspace => {
                    if current_workspace.num == min_workspace_num && max_workspace_num < 10 {
                        // At smallest possible workspace.
                        // Cycle back to one workspace after the max one
                        Workspace::Num(max_workspace_num + 1)
                    } else {
                        // Skip gaps when cycling between workspaces so the
                        // behaviour can be predictable
                        opt.to
                    }
                }
                Command::MoveContainerToWorkspace => {
                    // Unlike "prev", don't skip gaps between non-contiguous workspaces
                    // Also unlike prev, loop back to max + 1 rather than map
                    if current_workspace.num == 1 {
                        // Don't create workspaces with num < 1 or sway gets confused instead, create a new
                        // workspace after the max one
                        Workspace::Num(max_workspace_num + 1)
                    } else {
                        // Decrement without skipping gaps
                        Workspace::Num(current_workspace.num - 1)
                    }
                }
            }
        }
        Workspace::Num(_) => opt.to,
    }
}

#[derive(Debug)]
enum Command {
    MoveToWorkspace,
    MoveContainerToWorkspace,
}

impl FromStr for Command {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "workspace" => Ok(Self::MoveToWorkspace),
            "move-container-to-workspace" => Ok(Self::MoveContainerToWorkspace),
            _ => Err(format!("Can't parse {} as Command", s)),
        }
    }
}

#[derive(Debug, StructOpt)]
#[structopt(about = "Automatically create workspaces under sway like gnome does")]
struct Opt {
    #[structopt(default_value = "workspace")]
    command: Command,
    to: Workspace,
}

fn main() {
    pretty_env_logger::init();
    let opt = Opt::from_args();
    let mut wm = swayipc::Connection::new().unwrap();
    match opt.command {
        Command::MoveToWorkspace => {
            let destination = pick_destination(&mut wm, opt);
            wm.run_command(format!("workspace {}", destination))
                .unwrap();
        }
        Command::MoveContainerToWorkspace => {
            let destination = pick_destination(&mut wm, opt);
            wm.run_command(format!("move container to workspace {}", destination))
                .unwrap();
            wm.run_command(format!("workspace {}", destination))
                .unwrap();
        }
    }
}
