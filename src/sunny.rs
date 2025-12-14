use crate::config::Config;
use crate::mzn_to_fzn::convert_mzn;
use crate::scheduler::{Portfolio, Scheduler, SolverInfo};
use crate::{ai::Ai, args::Args};
use tokio::time::{Duration, sleep};

const FEATURES_SOLVER: &str = "gecode";

pub async fn sunny(args: Args, ai: impl Ai, config: Config) {
    let timer_duration = Duration::from_secs(config.dynamic_schedule_interval);
    let cores = args.cores.unwrap_or(2);
    let mut scheduler = Scheduler::new(&args, &config)
        .await
        .expect("Failed to create scheduler");
    scheduler.apply(static_schedule(cores)).await.unwrap(); // TODO: Maybe do this in another thread

    let mut timer = sleep(timer_duration);
    // let fzn = convert_mzn(
    //     &args.model,
    //     args.data.as_deref(),
    //     FEATURES_SOLVER,
    //     false,
    //     args.debug_verbosity,
    // )
    // .await
    // .expect("failed to initially convert .mzn to .fzn");

    // let features = fzn_to_features(&fzn)
    //     .await
    //     .expect("if we fail to get features, we can't run the AI and thus can't recover");

    loop {
        timer.await;
        // let schedule = ai.schedule(&vec![], cores);
        // scheduler
        //     .solver_manager
        //     .suspend_all_solvers()
        //     .await
        //     .unwrap();

        scheduler.apply(static_schedule(cores)).await.unwrap();
        // apply_schedule(&mut solver_manager, schedule).await;

        // solver_manager.suspend_all_solvers().await.unwrap();
        // solver_manager.resume_all_solvers().await.unwrap();

        // solver_manager.resume_solver(1).await.unwrap();
        // solver_manager.resume_solver(2).await.unwrap();
        // solver_manager.resume_solver(1).await.unwrap();
        // solver_manager.resume_solver(1).await.unwrap();
        // solver_manager.resume_solver(1).await.unwrap();

        timer = sleep(timer_duration);
    }
}

fn static_schedule(cores: usize) -> Portfolio {
    // let solvers = vec![
    //     "picat".to_string(),
    //     "gecode".to_string(),
    //     "cp-sat".to_string(),
    //     "chuffed".to_string(),
    //     "coinbc".to_string(),
    //     "yuck".to_string(),
    // ];
    // let mut schedule = vec![];
    // for (i, solver) in solvers.into_iter().cycle().take(100).enumerate() {
    //     schedule.push(ScheduleElement::new(i, solver, 1));
    // }
    // schedule
    vec![
        SolverInfo::new("coinbc".to_string(), 1),
        SolverInfo::new("gecode".to_string(), 1),
        SolverInfo::new("picat".to_string(), 1),
        SolverInfo::new("cp-sat".to_string(), 1),
        SolverInfo::new("chuffed".to_string(), 1),
        SolverInfo::new("yuck".to_string(), 1),
        // SolverInfo::new( "xpress".to_string(), cores / 10),
        // SolverInfo::new( "scip".to_string(), cores / 10),
        // SolverInfo::new( "highs".to_string(), cores / 10),
        // SolverInfo::new( "gurobi".to_string(), cores / 10),
        // SolverInfo::new("coinbc".to_string(), cores / 2),
    ]
}
