library("dplyr")
library("tidyr")
library("ggplot2")
library("readr")
library("purrr")
library("tibble")
library("stringr")
library("forcats")
library("lubridate")

fetch_rows <- function(url) {
  read.csv(url)
}
