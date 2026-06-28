# Flush persistent cache during compiler idle

Unpack will queue persistent cache writes and flush them during compiler idle instead of writing cache packs on the critical path of every cache store. Watch sessions enter idle after a compilation has completed, emitted assets have been written, and dependency sets are known; closing a compiler or watch session must wait for pending persistent cache writes or report their infrastructure error.
