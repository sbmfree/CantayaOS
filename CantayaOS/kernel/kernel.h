#ifndef KERNEL_H
#define KERNEL_H

#include <stdint.h>

// Kernel function prototypes
void kernel_main(void);
void initialize_memory(void);
void initialize_interrupts(void);
void initialize_devices(void);
void start_scheduler(void);

// Data structures
typedef struct {
    uint32_t id;
    uint32_t priority;
} task_t;

#endif // KERNEL_H