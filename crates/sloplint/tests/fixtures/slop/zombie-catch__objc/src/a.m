#import <Foundation/Foundation.h>

@interface Worker : NSObject
- (void)doThing;
- (void)runWithObj:(id)obj;
@end

@implementation Worker
- (void)doThing {
}

- (void)runWithObj:(id)obj {
    @try {
        [obj doThing];
    } @catch (NSException *e) {
    }
}
@end
