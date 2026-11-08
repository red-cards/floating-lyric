// Project Android mandiri (Gradle). Sengaja tidak tahu apa-apa soal Cargo
// workspace di root repo: tidak ada include ke ../crates.
pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
    }
}

rootProject.name = "MiniLyricsAndroid"
include(":app")
