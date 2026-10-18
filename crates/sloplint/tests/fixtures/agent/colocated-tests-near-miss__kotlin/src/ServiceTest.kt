import kotlin.test.Test
import kotlin.test.assertEquals

class ServiceTest {
    @Test
    fun testServe() {
        assertEquals("serving", serve())
    }
}
