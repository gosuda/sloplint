#import <Foundation/Foundation.h>

@interface Worker : NSObject
- (void)doThing;
- (void)handleError:(NSException *)e;
- (void)runWithObj:(id)obj;
@end

@implementation Worker
- (void)doThing {
}

- (void)handleError:(NSException *)e {
    NSLog(@"handled error: %@", e);
}

- (void)runWithObj:(id)obj {
    @try {
        [obj doThing];
    } @catch (NSException *e) {
        [self handleError:e];
    }
}
@end
