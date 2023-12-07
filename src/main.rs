use factoriosrc::{Config, ConfigError, World};

fn main() -> Result<(), ConfigError> {
    let config = Config::new(10, 10, 2);
    let mut world = World::new(config)?;

    world.search(None);

    println!("{}", world.rle(0));

    Ok(())
}
