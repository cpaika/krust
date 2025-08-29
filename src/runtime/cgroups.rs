// Cgroups v2 resource management for containers
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, error, info};

pub struct CgroupManager {
    cgroup_root: PathBuf,
}

impl CgroupManager {
    pub fn new() -> Result<Self, String> {
        // Check if cgroups v2 is mounted
        let cgroup_root = PathBuf::from("/sys/fs/cgroup");
        if !cgroup_root.exists() {
            return Err("Cgroups not mounted at /sys/fs/cgroup".to_string());
        }
        
        // Check if it's cgroups v2
        let cgroup_type = fs::read_to_string(cgroup_root.join("cgroup.controllers"))
            .map_err(|_| "Not running cgroups v2")?;
        
        debug!("Available cgroup controllers: {}", cgroup_type);
        
        Ok(Self { cgroup_root })
    }
    
    pub fn create_cgroup(&self, container_id: &str) -> Result<PathBuf, String> {
        let cgroup_path = self.cgroup_root.join("krust").join(container_id);
        fs::create_dir_all(&cgroup_path).map_err(|e| format!("Failed to create cgroup: {}", e))?;
        
        info!("Created cgroup at {:?}", cgroup_path);
        Ok(cgroup_path)
    }
    
    pub fn apply_resource_limits(
        &self,
        container_id: &str,
        memory_mb: Option<u64>,
        cpu_shares: Option<u64>,
        cpu_quota: Option<u64>,
        pids_limit: Option<u64>,
    ) -> Result<(), String> {
        let cgroup_path = self.cgroup_root.join("krust").join(container_id);
        
        // Apply memory limit
        if let Some(memory) = memory_mb {
            let memory_bytes = memory * 1024 * 1024;
            self.write_cgroup_file(&cgroup_path, "memory.max", &memory_bytes.to_string())?;
            
            // Also set swap to 0 to prevent swapping
            self.write_cgroup_file(&cgroup_path, "memory.swap.max", "0")?;
            
            info!("Applied memory limit: {} MB", memory);
        }
        
        // Apply CPU limits
        if let Some(shares) = cpu_shares {
            // CPU weight in cgroups v2 (1-10000, default 100)
            // Convert from docker-style shares (default 1024) to cgroups v2 weight
            let weight = (shares * 100) / 1024;
            self.write_cgroup_file(&cgroup_path, "cpu.weight", &weight.to_string())?;
            info!("Applied CPU weight: {}", weight);
        }
        
        if let Some(quota) = cpu_quota {
            // CPU quota in microseconds per period (default period is 100ms)
            let period = 100000; // 100ms in microseconds
            let cpu_max = format!("{} {}", quota, period);
            self.write_cgroup_file(&cgroup_path, "cpu.max", &cpu_max)?;
            info!("Applied CPU quota: {} us per {} us", quota, period);
        }
        
        // Apply PID limit
        if let Some(pids) = pids_limit {
            self.write_cgroup_file(&cgroup_path, "pids.max", &pids.to_string())?;
            info!("Applied PID limit: {}", pids);
        }
        
        Ok(())
    }
    
    pub fn add_process_to_cgroup(&self, container_id: &str, pid: u32) -> Result<(), String> {
        let cgroup_path = self.cgroup_root.join("krust").join(container_id);
        self.write_cgroup_file(&cgroup_path, "cgroup.procs", &pid.to_string())?;
        
        info!("Added PID {} to cgroup {}", pid, container_id);
        Ok(())
    }
    
    pub fn get_memory_usage(&self, container_id: &str) -> Result<u64, String> {
        let cgroup_path = self.cgroup_root.join("krust").join(container_id);
        let usage_str = fs::read_to_string(cgroup_path.join("memory.current"))
            .map_err(|e| format!("Failed to read memory usage: {}", e))?;
        
        usage_str
            .trim()
            .parse()
            .map_err(|e| format!("Failed to parse memory usage: {}", e))
    }
    
    pub fn get_cpu_usage(&self, container_id: &str) -> Result<CpuStats, String> {
        let cgroup_path = self.cgroup_root.join("krust").join(container_id);
        let stat_str = fs::read_to_string(cgroup_path.join("cpu.stat"))
            .map_err(|e| format!("Failed to read CPU stats: {}", e))?;
        
        let mut usage_usec = 0u64;
        let mut user_usec = 0u64;
        let mut system_usec = 0u64;
        
        for line in stat_str.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() == 2 {
                match parts[0] {
                    "usage_usec" => usage_usec = parts[1].parse().unwrap_or(0),
                    "user_usec" => user_usec = parts[1].parse().unwrap_or(0),
                    "system_usec" => system_usec = parts[1].parse().unwrap_or(0),
                    _ => {}
                }
            }
        }
        
        Ok(CpuStats {
            usage_usec,
            user_usec,
            system_usec,
        })
    }
    
    pub fn cleanup_cgroup(&self, container_id: &str) -> Result<(), String> {
        let cgroup_path = self.cgroup_root.join("krust").join(container_id);
        
        // First, ensure no processes are in the cgroup
        let procs = fs::read_to_string(cgroup_path.join("cgroup.procs"))
            .unwrap_or_default();
        
        if !procs.trim().is_empty() {
            error!("Cgroup {} still has processes: {}", container_id, procs);
            return Err("Cgroup still has processes".to_string());
        }
        
        // Remove the cgroup directory
        fs::remove_dir(&cgroup_path).map_err(|e| format!("Failed to remove cgroup: {}", e))?;
        
        info!("Cleaned up cgroup for container {}", container_id);
        Ok(())
    }
    
    fn write_cgroup_file(&self, cgroup_path: &Path, file: &str, value: &str) -> Result<(), String> {
        let file_path = cgroup_path.join(file);
        fs::write(&file_path, value)
            .map_err(|e| format!("Failed to write to {:?}: {}", file_path, e))?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct CpuStats {
    pub usage_usec: u64,
    pub user_usec: u64,
    pub system_usec: u64,
}

// Helper functions for cgroups v1 compatibility
pub fn is_cgroups_v2() -> bool {
    Path::new("/sys/fs/cgroup/cgroup.controllers").exists()
}

pub fn get_available_controllers() -> Result<Vec<String>, String> {
    if !is_cgroups_v2() {
        return Err("System is not running cgroups v2".to_string());
    }
    
    let controllers = fs::read_to_string("/sys/fs/cgroup/cgroup.controllers")
        .map_err(|e| format!("Failed to read controllers: {}", e))?;
    
    Ok(controllers.split_whitespace().map(String::from).collect())
}