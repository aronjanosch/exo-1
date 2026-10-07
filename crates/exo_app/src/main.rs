fn main() -> bevy::app::AppExit {
    let o = exo_app::Options::from_args(std::env::args().skip(1));
    exo_app::build_app(&o).run()
}
