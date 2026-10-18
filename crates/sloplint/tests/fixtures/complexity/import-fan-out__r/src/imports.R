library("dplyr")
library("tidyr")
library("ggplot2")
library("readr")
library("purrr")
library("tibble")
library("stringr")
library("forcats")
library("lubridate")
library("httr")
library("jsonlite")
library("yaml")
library("xml2")
library("rvest")
library("shiny")

fetch_rows <- function(url) {
  read.csv(url)
}
