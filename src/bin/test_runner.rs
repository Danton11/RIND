use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use std::net::{TcpStream, UdpSocket};


fn check_server_connectivity() -> bool {
    // Check DNS port (UDP)
    if let Ok(socket) = UdpSocket::bind("0.0.0.0:0") {
        if socket.connect("127.0.0.1:12312").is_ok() {
            // Check API port (TCP)
            return TcpStream::connect("127.0.0.1:8080").is_ok();
        }
    }
    false
}

fn run_command(name: &str, command: &str, args: &[&str]) -> (bool, Duration, String) {
    println!("\n{}", "=".repeat(60));
    println!("🧪 Running {}", name);
    println!("{}", "=".repeat(60));

    let start = Instant::now();
    
    let output = Command::new(command)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();

    let duration = start.elapsed();

    match output {
        Ok(output) => {
            let success = output.status.success();
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            
            if success {
                println!("✅ {} PASSED ({:.2}s)", name, duration.as_secs_f64());
                if !stdout.is_empty() {
                    println!("{}", stdout);
                }
            } else {
                println!("❌ {} FAILED ({:.2}s)", name, duration.as_secs_f64());
                if !stderr.is_empty() {
                    println!("Error: {}", stderr);
                }
                if !stdout.is_empty() {
                    println!("Output: {}", stdout);
                }
            }
            
            (success, duration, format!("{}{}", stdout, stderr))
        }
        Err(e) => {
            println!("💥 {} EXCEPTION: {}", name, e);
            (false, duration, e.to_string())
        }
    }
}

fn main() {
    println!("🚀 DNS Server Test Suite (Rust Edition)");
    println!("{}", "=".repeat(60));

    // Check server connectivity
    println!("🔍 Checking server connectivity...");
    if !check_server_connectivity() {
        println!("❌ DNS server is not running or not accessible!");
        println!("\nPlease start the server first:");
        println!("  cargo run");
        println!("\nServer should be listening on:");
        println!("  - DNS: UDP 127.0.0.1:12312");
        println!("  - API: HTTP 127.0.0.1:8080");
        std::process::exit(1);
    }
    println!("✅ Server is accessible");

    let total_start = Instant::now();
    let mut results = Vec::new();

    // Run Rust unit tests
    let (success, duration, output) = run_command(
        "Unit Tests",
        "cargo",
        &["test", "--lib", "--tests"]
    );
    results.push(("Unit Tests", success, duration, output));

    // Run integration tests
    let (success, duration, output) = run_command(
        "Integration Tests", 
        "cargo",
        &["test", "--test", "integration_tests"]
    );
    results.push(("Integration Tests", success, duration, output));

    // Run benchmarks (just to verify they compile and run)
    let (success, duration, output) = run_command(
        "Benchmarks",
        "cargo",
        &["bench", "--bench", "dns_benchmarks", "--", "--test"]
    );
    results.push(("Benchmarks", success, duration, output));

    let total_duration = total_start.elapsed();

    // Print summary
    println!("\n{}", "=".repeat(60));
    println!("📊 TEST SUMMARY");
    println!("{}", "=".repeat(60));

    let passed = results.iter().filter(|(_, success, _, _)| *success).count();
    let failed = results.len() - passed;

    println!("Total Tests: {}", results.len());
    println!("✅ Passed: {}", passed);
    println!("❌ Failed: {}", failed);
    println!("⏱️  Total Time: {:.2}s", total_duration.as_secs_f64());

    println!("\nDetailed Results:");
    for (name, success, duration, error) in &results {
        let status = if *success { "✅ PASS" } else { "❌ FAIL" };
        println!("  {} {:<20} ({:.2}s)", status, name, duration.as_secs_f64());
        if !*success && !error.is_empty() {
            let error_preview = error.lines().next().unwrap_or("Unknown error");
            println!("       Error: {}...", &error_preview[..error_preview.len().min(80)]);
        }
    }

    // Overall result
    if failed == 0 {
        println!("\n🎉 ALL TESTS PASSED!");
        println!("Your DNS server is performing excellently!");
        std::process::exit(0);
    } else {
        println!("\n⚠️  {} TEST(S) FAILED", failed);
        println!("Check the detailed output above for issues.");
        std::process::exit(1);
    }
}