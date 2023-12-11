use factoriosrc_lib::{Config, ConfigError, World};

fn main() -> Result<(), ConfigError> {
    let config = Config::new(5, 5, 2);
    let mut world = World::new(config)?;

    world.search(None);

    println!("{}", world.rle(0));

    Ok(())
}
