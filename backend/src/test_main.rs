use std::time::Instant;

#[tokio::main]
async fn main() {
    let start = Instant::now();
    
    println!("Step 1: Starting logging init...");
    novaclaw_backend::logging::init();
    println!("Step 1 done: {:?}", start.elapsed());
    
    println!("Step 2: Starting initialize...");
    novaclaw_backend::initialize().await;
    println!("Step 2 done: {:?}", start.elapsed());
    
    println!("Step 3: Starting server...");
    novaclaw_backend::server::start().await;
}
