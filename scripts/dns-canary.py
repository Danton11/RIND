#!/usr/bin/env python3
"""
DNS Canary Script - Generates realistic DNS traffic for testing and monitoring

This script sends DNS queries to the RIND DNS server with:
- Random domains (both existing and non-existing)
- Random query types (A, AAAA, MX, CNAME, TXT, NS)
- Random intervals between queries
- Expected response codes (NOERROR, NXDOMAIN, SERVFAIL)
"""

import random
import time
import subprocess
import sys
import signal
import logging
from typing import List, Tuple
from datetime import datetime

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(levelname)s - %(message)s',
    handlers=[
        logging.FileHandler('dns-canary.log'),
        logging.StreamHandler(sys.stdout)
    ]
)
logger = logging.getLogger(__name__)

class DNSCanary:
    def __init__(self, dns_server="localhost", dns_port=12312):
        self.dns_server = dns_server
        self.dns_port = dns_port
        self.running = True
        
        # Domains that should exist in the DNS server (based on dns_records.txt)
        self.existing_domains = [
            "example.com",
            "test.com", 
            "google.com",
            "amazon.com",
            "github.com"
        ]
        
        # Random domains that likely don't exist
        self.nonexistent_domains = [
            "nonexistent-domain-12345.com",
            "fake-website-xyz.net",
            "does-not-exist-abc.org",
            "random-gibberish-domain.info",
            "test-nxdomain-response.example",
            "missing-record.test",
            "invalid-hostname-999.local"
        ]
        
        # DNS query types to test
        self.query_types = ["A", "AAAA", "MX", "CNAME", "TXT", "NS"]
        
        # Query patterns with expected responses
        self.query_patterns = [
            # Existing domains - should return NOERROR
            {"domains": self.existing_domains, "weight": 60, "expected": "NOERROR"},
            # Non-existent domains - should return NXDOMAIN  
            {"domains": self.nonexistent_domains, "weight": 30, "expected": "NXDOMAIN"},
            # Edge cases that might cause SERVFAIL
            {"domains": ["", ".", "invalid..domain", "toolong" + "x" * 250 + ".com"], "weight": 10, "expected": "SERVFAIL"}
        ]
        
        # Statistics tracking
        self.stats = {
            "total_queries": 0,
            "noerror_responses": 0,
            "nxdomain_responses": 0,
            "servfail_responses": 0,
            "timeout_responses": 0,
            "other_responses": 0
        }

    def signal_handler(self, signum, frame):
        """Handle graceful shutdown on SIGINT/SIGTERM"""
        logger.info("Received shutdown signal, stopping canary...")
        self.running = False

    def select_query(self) -> Tuple[str, str]:
        """Select a random domain and query type based on weighted patterns"""
        # Choose pattern based on weights
        total_weight = sum(p["weight"] for p in self.query_patterns)
        rand = random.randint(1, total_weight)
        
        cumulative = 0
        selected_pattern = None
        for pattern in self.query_patterns:
            cumulative += pattern["weight"]
            if rand <= cumulative:
                selected_pattern = pattern
                break
        
        # Select random domain from the chosen pattern
        domain = random.choice(selected_pattern["domains"])
        query_type = random.choice(self.query_types)
        
        return domain, query_type

    def send_dns_query(self, domain: str, query_type: str) -> str:
        """Send DNS query using dig and return the response code"""
        try:
            cmd = [
                "dig", 
                f"@{self.dns_server}", 
                "-p", str(self.dns_port),
                "+short",
                "+time=2",  # 2 second timeout
                "+tries=1", # Only try once
                domain, 
                query_type
            ]
            
            # Run dig command
            result = subprocess.run(
                cmd, 
                capture_output=True, 
                text=True, 
                timeout=5
            )
            
            # Parse response code from dig output
            if result.returncode == 0:
                return "NOERROR"
            elif result.returncode == 9:  # NXDOMAIN
                return "NXDOMAIN"
            elif result.returncode == 2:   # SERVFAIL
                return "SERVFAIL"
            else:
                return "OTHER"
                
        except subprocess.TimeoutExpired:
            return "TIMEOUT"
        except Exception as e:
            logger.error(f"Error sending DNS query: {e}")
            return "ERROR"

    def update_stats(self, response_code: str):
        """Update statistics based on response code"""
        self.stats["total_queries"] += 1
        
        if response_code == "NOERROR":
            self.stats["noerror_responses"] += 1
        elif response_code == "NXDOMAIN":
            self.stats["nxdomain_responses"] += 1
        elif response_code == "SERVFAIL":
            self.stats["servfail_responses"] += 1
        elif response_code == "TIMEOUT":
            self.stats["timeout_responses"] += 1
        else:
            self.stats["other_responses"] += 1

    def log_stats(self):
        """Log current statistics"""
        total = self.stats["total_queries"]
        if total == 0:
            return
            
        logger.info(f"Stats - Total: {total}, "
                   f"NOERROR: {self.stats['noerror_responses']} ({self.stats['noerror_responses']/total*100:.1f}%), "
                   f"NXDOMAIN: {self.stats['nxdomain_responses']} ({self.stats['nxdomain_responses']/total*100:.1f}%), "
                   f"SERVFAIL: {self.stats['servfail_responses']} ({self.stats['servfail_responses']/total*100:.1f}%), "
                   f"TIMEOUT: {self.stats['timeout_responses']} ({self.stats['timeout_responses']/total*100:.1f}%)")

    def run(self):
        """Main canary loop"""
        logger.info(f"Starting DNS Canary - targeting {self.dns_server}:{self.dns_port}")
        
        # Set up signal handlers for graceful shutdown
        signal.signal(signal.SIGINT, self.signal_handler)
        signal.signal(signal.SIGTERM, self.signal_handler)
        
        last_stats_log = time.time()
        
        while self.running:
            try:
                # Select random domain and query type
                domain, query_type = self.select_query()
                
                # Send DNS query
                start_time = time.time()
                response_code = self.send_dns_query(domain, query_type)
                duration = time.time() - start_time
                
                # Update statistics
                self.update_stats(response_code)
                
                # Log query details
                logger.debug(f"Query: {domain} {query_type} -> {response_code} ({duration:.3f}s)")
                
                # Log stats every 60 seconds
                if time.time() - last_stats_log > 60:
                    self.log_stats()
                    last_stats_log = time.time()
                
                # Random sleep between queries (0.5 to 5 seconds)
                sleep_time = random.uniform(0.01, 0.02)
                time.sleep(sleep_time)
                
            except KeyboardInterrupt:
                break
            except Exception as e:
                logger.error(f"Unexpected error in canary loop: {e}")
                time.sleep(1)
        
        # Final stats
        logger.info("DNS Canary stopped")
        self.log_stats()

def main():
    """Main entry point"""
    import argparse
    
    parser = argparse.ArgumentParser(description="DNS Canary - Generate realistic DNS traffic")
    parser.add_argument("--server", default="localhost", help="DNS server address (default: localhost)")
    parser.add_argument("--port", type=int, default=12312, help="DNS server port (default: 12312)")
    parser.add_argument("--verbose", "-v", action="store_true", help="Enable verbose logging")
    
    args = parser.parse_args()
    
    if args.verbose:
        logging.getLogger().setLevel(logging.DEBUG)
    
    canary = DNSCanary(dns_server=args.server, dns_port=args.port)
    canary.run()

if __name__ == "__main__":
    main()