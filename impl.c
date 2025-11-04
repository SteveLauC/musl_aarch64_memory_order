#include <aio.h>
#include <errno.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <time.h>
#include <assert.h>

// This global flag is the C equivalent of your AtomicBool.
// `volatile sig_atomic_t` is the correct type for variables
// modified in a signal handler and read elsewhere.
volatile sig_atomic_t signaled = 0;

// The signal handler, equivalent to your `sigfunc`.
void sigusr2_handler(int signum) {
    // This write is async-signal-safe, just like in your Rust test.
    const char* msg = "DBG: C version signaled\n";
    write(STDOUT_FILENO, msg, strlen(msg));
    signaled = 1;
}

int main() {
    // 1. Set up signal handler
    struct sigaction sa;
    memset(&sa, 0, sizeof(sa));
    sa.sa_handler = sigusr2_handler;
    // SA_RESETHAND mimics the behavior in your Rust code, resetting the
    // handler to default after it's called once.
    sa.sa_flags = SA_RESETHAND; 
    sigemptyset(&sa.sa_mask);

    if (sigaction(SIGUSR2, &sa, NULL) == -1) {
        perror("sigaction failed");
        return 1;
    }

    // 2. Create a temporary file
    FILE* temp_f = tmpfile();
    if (temp_f == NULL) {
        perror("tmpfile failed");
        return 1;
    }
    // Get the file descriptor
    int fd = fileno(temp_f);

    // 3. Set up the AIO control block (aiocb)
    const char* wbuf = "abcdef123456";
    struct aiocb aio_cb;
    memset(&aio_cb, 0, sizeof(aio_cb));

    aio_cb.aio_fildes = fd;
    aio_cb.aio_offset = 2;
    aio_cb.aio_buf = (void*)wbuf;
    aio_cb.aio_nbytes = strlen(wbuf);
    aio_cb.aio_reqprio = 0;
    
    // Note: When using lio_listio, the notification comes from the
    // sigevent passed to lio_listio itself, not this one.
    // We leave it as SIGEV_NONE here.
    aio_cb.aio_sigevent.sigev_notify = SIGEV_NONE;

    // 4. Set up the notification for the lio_listio call
    struct sigevent lio_sev;
    memset(&lio_sev, 0, sizeof(lio_sev));
    lio_sev.sigev_notify = SIGEV_SIGNAL;
    lio_sev.sigev_signo = SIGUSR2;

    // 5. Submit the AIO operation using lio_listio
    struct aiocb* list_of_aiocbs[1] = { &aio_cb };

    printf("Submitting AIO request...\n");
    if (lio_listio(LIO_NOWAIT, list_of_aiocbs, 1, &lio_sev) == -1) {
        perror("lio_listio failed");
        fclose(temp_f);
        return 1;
    }

    // 6. Wait for the signal
    printf("Waiting for signal...\n");
    struct timespec sleep_duration = {0, 10 * 1000 * 1000}; // 10 milliseconds
    while (!signaled) {
        nanosleep(&sleep_duration, NULL);
    }
    printf("Signal received, loop exited.\n");

    // 7. Verify the result
    ssize_t return_status = aio_return(&aio_cb);
    if (return_status == -1) {
        perror("aio_return failed");
        fclose(temp_f);
        return 1;
    }

    printf("aio_return() reported %zd bytes written.\n", return_status);
    assert(return_status == strlen(wbuf));

    printf("C test completed successfully.\n");

    // tmpfile() automatically removes the file on close.
    fclose(temp_f);

    return 0;
}
