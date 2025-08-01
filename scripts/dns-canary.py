#!/usr/bin/env python3
"""
RIND DNS Canary Monitoring Script
Continuously monitors DNS server health and exposes metrics
"""

import time
import socket
import requests
import threading
import argparse
import logging
import json
from http.server import HTTPServer, BaseHTTPRequestHandler
from urllib.parse import urlparse
import signal
import sys

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(levelname)s - %(message)s'
)
logger = logging.getLogger(__name__)

class DNSCanaryMetrics:
    """Collects and stores DNS canary metrics"""
    
    def __init__(self):
        self.start_time = time.time()
        self.dns_queries_total = 0
        self.dns_queries_success = 0
        self.dns_queries_failed = 0
        self.dns_response_time_sum = 0.0
        self.dns_last_response_time = 0.0
        self.api_requests_total = 0
        self.api_requests_success = 0
        self.api_requests_failed = 0
        self.api_response_time_sum = 0.0
        self.api_last_response_time = 0.0
        self.dns_server_up = 0
        self.api_server_up = 0
        self.lock = threading.Lock()
    
    def record_dns_query(self, success, response_time):
        with self.lock:
            self.dns_queries_total += 1
            if success:
                self.dns_queries_success += 1
                self.dns_server_up = 1
            else:
                self.dns_queries_failed += 1
                self.dns_server_up = 0
            self.dns_response_time_sum += response_time
            self.dns_last_response_time = response_time
    
    def record_api_request(self, success, response_time):
        with self.lock:
            self.api_requests_total += 1
            if success:
                self.api_requests_success += 1
                self.api_server_up = 1
            else:
                self.api_requests_failed += 1
                self.api_server_up = 0
            self.api_response_time_sum += response_time
            self.api_last_response_time = response_time
    
    def get_metrics(self):
        with self.lock:
            uptime = time.time() - self.start_time
            dns_avg_response_time = (self.dns_response_time_sum / self.dns_queries_total) if self.dns_queries_total > 0 else 0
            api_avg_response_time = (self.api_response_time_sum / self.api_requests_total) if self.api_requests_total > 0 else 0
            dns_success_rate = (self.dns_queries_success / self.dns_queries_total * 100) if self.dns_queries_total > 0 else 0
            api_success_rate = (self.api_requests_success / self.api_requests_total * 100) if self.api_requests_total > 0 else 0
            
            return {
                'canary_uptime_seconds': uptime,
                'canary_dns_queries_total': self.dns_queries_total,
                'canary_dns_queries_success_total': self.dns_queries_success,
                'canary_dns_queries_failed_total': self.dns_queries_failed,
                'canary_dns_response_time_seconds': self.dns_last_response_time,
                'canary_dns_response_time_avg_seconds': dns_avg_response_time,
                'canary_dns_success_rate_percent': dns_success_rate,
                'canary_dns_server_up': self.dns_server_up,
                'canary_api_requests_total': self.api_requests_total,
                'canary_api_requests_success_total': self.api_requests_success,
                'canary_api_requests_failed_total': self.api_requests_failed,
                'canary_api_response_time_seconds': self.api_last_response_time,
                'canary_api_response_time_avg_seconds': api_avg_response_time,
                'canary_api_success_rate_percent': api_success_rate,
                'canary_api_server_up': self.api_server_up
            }

class MetricsHandler(BaseHTTPRequestHandler):
    """HTTP handler for metrics endpoint"""
    
    def __init__(self, *args, metrics=None, **kwargs):
        self.metrics = metrics
        super().__init__(*args, **kwargs)
    
    def do_GET(self):
        parsed_path = urlparse(self.path)
        
        if parsed_path.path == '/metrics':
            self.send_response(200)
            self.send_header('Content-Type', 'text/plain; charset=utf-8')
            self.end_headers()
            
            metrics_data = self.metrics.get_metrics()
            output = []
            output.append('# HELP canary_metrics DNS Canary monitoring metrics')
            output.append('# TYPE canary_metrics gauge')
            
            for metric_name, value in metrics_data.items():
                output.append(f'{metric_name} {value}')
            
            self.wfile.write('\n'.join(output).encode('utf-8'))
            self.wfile.write(b'\n')
            
        elif parsed_path.path == '/health':
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.end_headers()
            
            health_data = {
                'status': 'healthy',
                'timestamp': time.time(),
                'uptime': time.time() - self.metrics.start_time
            }
            self.wfile.write(json.dumps(health_data).encode('utf-8'))
            
        else:
            self.send_response(404)
            self.end_headers()
            self.wfile.write(b'Not Found')
    
    def do_HEAD(self):
        parsed_path = urlparse(self.path)
        
        if parsed_path.path in ['/health', '/metrics']:
            self.send_response(200)
            self.send_header('Content-Type', 'text/plain' if parsed_path.path == '/metrics' else 'application/json')
            self.end_headers()
        else:
            self.send_response(404)
            self.end_headers()
    
    def log_message(self, format, *args):
        # Suppress default HTTP logging
        pass

class DNSCanary:
    """Main DNS canary monitoring class"""
    
    def __init__(self, dns_server, dns_port, api_server, api_port, metrics_port):
        self.dns_server = dns_server
        self.dns_port = dns_port
        self.api_server = api_server
        self.api_port = api_port
        self.metrics_port = metrics_port
        self.metrics = DNSCanaryMetrics()
        self.running = True
        self.test_domains = ['example.com', 'google.com', 'cloudflare.com']
        
    def test_dns_query(self, domain):
        """Test a DNS query"""
        try:
            start_time = time.time()
            
            # Create UDP socket
            sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
            sock.settimeout(5.0)
            
            # Simple DNS query packet for A record
            query_id = 0x1234
            query = bytearray()
            query.extend(query_id.to_bytes(2, 'big'))  # ID
            query.extend(b'\x01\x00')  # Flags: standard query
            query.extend(b'\x00\x01')  # Questions: 1
            query.extend(b'\x00\x00')  # Answer RRs: 0
            query.extend(b'\x00\x00')  # Authority RRs: 0
            query.extend(b'\x00\x00')  # Additional RRs: 0
            
            # Encode domain name
            for part in domain.split('.'):
                query.append(len(part))
                query.extend(part.encode())
            query.append(0)  # End of name
            
            query.extend(b'\x00\x01')  # Type: A
            query.extend(b'\x00\x01')  # Class: IN
            
            # Send query
            sock.sendto(query, (self.dns_server, self.dns_port))
            
            # Receive response
            response, addr = sock.recvfrom(512)
            sock.close()
            
            response_time = time.time() - start_time
            
            # Basic validation - check if we got a response
            if len(response) >= 12:
                return True, response_time
            else:
                return False, response_time
                
        except Exception as e:
            response_time = time.time() - start_time
            logger.debug(f"DNS query failed for {domain}: {e}")
            return False, response_time
    
    def test_api_request(self):
        """Test an API request"""
        try:
            start_time = time.time()
            url = f"http://{self.api_server}:{self.api_port}/records"
            
            response = requests.get(url, timeout=5)
            response_time = time.time() - start_time
            
            if response.status_code == 200:
                return True, response_time
            else:
                return False, response_time
                
        except Exception as e:
            response_time = time.time() - start_time
            logger.debug(f"API request failed: {e}")
            return False, response_time
    
    def monitoring_loop(self):
        """Main monitoring loop"""
        logger.info("Starting DNS canary monitoring loop")
        
        while self.running:
            try:
                # Test DNS queries
                for domain in self.test_domains:
                    success, response_time = self.test_dns_query(domain)
                    self.metrics.record_dns_query(success, response_time)
                    
                    if success:
                        logger.debug(f"DNS query for {domain}: {response_time:.3f}s")
                    else:
                        logger.warning(f"DNS query failed for {domain}: {response_time:.3f}s")
                
                # Test API request
                success, response_time = self.test_api_request()
                self.metrics.record_api_request(success, response_time)
                
                if success:
                    logger.debug(f"API request: {response_time:.3f}s")
                else:
                    logger.warning(f"API request failed: {response_time:.3f}s")
                
                # Wait before next test cycle
                time.sleep(10)
                
            except Exception as e:
                logger.error(f"Error in monitoring loop: {e}")
                time.sleep(5)
    
    def start_metrics_server(self):
        """Start the metrics HTTP server"""
        def create_handler(*args, **kwargs):
            return MetricsHandler(*args, metrics=self.metrics, **kwargs)
        
        server = HTTPServer(('0.0.0.0', self.metrics_port), create_handler)
        logger.info(f"Metrics server starting on port {self.metrics_port}")
        
        def serve_forever():
            try:
                server.serve_forever()
            except Exception as e:
                logger.error(f"Metrics server error: {e}")
        
        metrics_thread = threading.Thread(target=serve_forever, daemon=True)
        metrics_thread.start()
        
        return server
    
    def start(self):
        """Start the canary monitoring"""
        logger.info(f"Starting DNS canary monitoring")
        logger.info(f"DNS Server: {self.dns_server}:{self.dns_port}")
        logger.info(f"API Server: {self.api_server}:{self.api_port}")
        logger.info(f"Metrics Port: {self.metrics_port}")
        
        # Start metrics server
        metrics_server = self.start_metrics_server()
        
        # Setup signal handlers
        def signal_handler(signum, frame):
            logger.info("Received shutdown signal")
            self.running = False
            metrics_server.shutdown()
            sys.exit(0)
        
        signal.signal(signal.SIGINT, signal_handler)
        signal.signal(signal.SIGTERM, signal_handler)
        
        # Start monitoring loop
        try:
            self.monitoring_loop()
        except KeyboardInterrupt:
            logger.info("Shutting down canary monitoring")
            self.running = False
            metrics_server.shutdown()

def main():
    parser = argparse.ArgumentParser(description='RIND DNS Canary Monitoring')
    parser.add_argument('--dns-server', default='localhost', help='DNS server hostname')
    parser.add_argument('--dns-port', type=int, default=12312, help='DNS server port')
    parser.add_argument('--api-server', default='localhost', help='API server hostname')
    parser.add_argument('--api-port', type=int, default=8080, help='API server port')
    parser.add_argument('--metrics-port', type=int, default=8090, help='Metrics server port')
    parser.add_argument('--log-level', default='INFO', help='Log level')
    
    args = parser.parse_args()
    
    # Set log level
    logging.getLogger().setLevel(getattr(logging, args.log_level.upper()))
    
    # Create and start canary
    canary = DNSCanary(
        args.dns_server,
        args.dns_port,
        args.api_server,
        args.api_port,
        args.metrics_port
    )
    
    canary.start()

if __name__ == '__main__':
    main()