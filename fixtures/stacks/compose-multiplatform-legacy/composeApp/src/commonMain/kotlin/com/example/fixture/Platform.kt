package com.example.fixture

interface Platform {
    val name: String
}

expect fun getPlatform(): Platform