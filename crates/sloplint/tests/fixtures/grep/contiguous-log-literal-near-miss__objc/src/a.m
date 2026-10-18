#import <Foundation/Foundation.h>

void ReportFailure(const char *path) {
    NSLog(@"failed to open file: %@", path);
}
