use std::collections::VecDeque;
use std::net::Ipv4Addr;
use std::sync::Mutex;

pub struct IpPool {
    pub subnet_base: Ipv4Addr,
    pub subnet_prefix: u8,
    available: Mutex<VecDeque<Ipv4Addr>>,
}

impl IpPool {
    pub fn new(subnet_base: Ipv4Addr, subnet_prefix: u8) -> Self {
        let base_u32 = u32::from(subnet_base);
        let host_bits = 32 - subnet_prefix as u32;
        let num_addrs = 1u32 << host_bits;

        // skip network addr (0), server/gateway (1), and broadcast (last)
        let mut queue = VecDeque::new();
        for i in 2..(num_addrs - 1) {
            queue.push_back(Ipv4Addr::from(base_u32 + i));
        }

        IpPool {
            subnet_base,
            subnet_prefix,
            available: Mutex::new(queue),
        }
    }

    pub fn server_ip(&self) -> Ipv4Addr {
        let base_u32 = u32::from(self.subnet_base);
        return Ipv4Addr::from(base_u32 + 1);
    }

    pub fn allocate(&self) -> Option<Ipv4Addr> {
        self.available.lock().unwrap().pop_front()
    }

    pub fn release(&self, ip: Ipv4Addr) {
        self.available.lock().unwrap().push_back(ip);
    }
}
